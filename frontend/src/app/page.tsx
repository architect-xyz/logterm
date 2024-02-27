"use client";

import {useCallback, useEffect, useRef, useState} from "react";
import {produce} from "immer";
import {useDebounce, useMeasure, usePrevious} from "@uidotdev/usehooks";
import useWebSocket, { ReadyState } from 'react-use-websocket';
import { Listbox } from '@headlessui/react'
import AutoSizer from "react-virtualized-auto-sizer";
import { FixedSizeList } from "react-window";
import styles from "./page.module.css";

type Logs = {
  total_display_lines: number,
  display_lines: DisplayLine[],
};

type DisplayLine = {
  lln: number,
  ll?: number | null,
  ts?: Date | null,
  spans: DisplaySpan[],
};

type DisplaySpan = {
  text: string,
  label: string
}

export default function Home() {
  // filter
  const filterRef = useRef<HTMLInputElement>(null);
  const [filter, setFilter] = useState<string | null>(null);
  const updateFilter = useCallback(() => {
    setFilter(filterRef.current?.value ?? null);
  }, []);
  const clearFilter = useCallback(() => {
    if (filterRef.current) {
      filterRef.current.value = '';
      updateFilter();
    }
  }, [updateFilter]);
  // select
  const [logSets, setLogSets] = useState<string[]>([]);
  const [selectedLogSet, setSelectedLogSet] = useState<string | null>(null);
  useEffect(() => {
    if (!selectedLogSet && logSets && logSets.length > 0) {
      setSelectedLogSet(logSets[0]);
    }
  }, [logSets, selectedLogSet]);
  // jsonrpc-over-websocket handling
  const [socketUrl] = useState('ws://127.0.0.1:9000');
  const { sendMessage, lastMessage, readyState } = useWebSocket(socketUrl);
  const nextRequestId = useRef(0);
  const inFlightRequests = useRef<{ [id: number]: string }>({});
  const [data, setData] = useState<Logs>({
    total_display_lines: 0,
    display_lines: [],
  });
  useEffect(() => {
    const data = lastMessage?.data;
    if (data) {
      const response = JSON.parse(data);
      const id = response["id"] as number;
      const method = inFlightRequests.current[id];
      if (method) {
        delete inFlightRequests.current[id];
        if (method === "list") {
          setLogSets(response["result"]);
        }
      } else if (response["method"] === "tail") {
        const params = response["params"];
        setData(produce((data) => {
          data.total_display_lines += params.display_lines.length;
          data.display_lines.push(...params.display_lines);
        }));
      } else if (response["method"] === "done") {
        console.warn("file done");
      }
    }
  }, [inFlightRequests, lastMessage]);
  // display measurement
  const [ref, { width: lineWidth, height }] = useMeasure();
  const [ruler, { width: charWidth, height: charHeight }] = useMeasure();
  const cols = useDebounce(lineWidth && charWidth && Math.floor(lineWidth / charWidth), 300);
  const rows = useDebounce(height && charHeight && Math.floor(height / charHeight), 300);
  // reload logs on resize
  const reloadLog = useCallback((logset: string, cols: number, filter?: string | null) => {
    if (cols) {
      const requestId = nextRequestId.current++;
      const method = "logs";
      inFlightRequests.current[requestId] = method;
      sendMessage(JSON.stringify({
        id: requestId,
        method,
        params: {
          logset,
          cols,
          filter: filter && filter.length > 0 ? filter : undefined,
        }
      }));
    }
  }, [sendMessage]);
  useEffect(() => {
    if (selectedLogSet && cols) {
      reloadLog(selectedLogSet, cols, filter);
    }
  }, [reloadLog, selectedLogSet, cols, filter]);
  // list logsets on startup
  useEffect(() => {
    const requestId = nextRequestId.current++;
    const method = "list";
    inFlightRequests.current[requestId] = method;
    sendMessage(JSON.stringify({
      id: requestId,
      method,
    }));
  }, [sendMessage]);
  const prevSelectedLogSet = usePrevious(selectedLogSet);
  const prevFilter = usePrevious(filter);
  useEffect(() => {
    if (selectedLogSet !== prevSelectedLogSet || filter !== prevFilter) {
      setData({
        total_display_lines: 0,
        display_lines: [],
      });
    }
  }, [selectedLogSet, prevSelectedLogSet, filter, prevFilter]);
  // react-window state
  const windowRef = useRef<FixedSizeList>(null);
  const [visibleStartIndex, setVisibleStartIndex] = useState(0);
  const [visibleEndIndex, setVisibleEndIndex] = useState(0);
  const isTailing = visibleEndIndex == data.total_display_lines - 1;
  const wasTailing = usePrevious(isTailing);
  const prevTotalDisplayLines = usePrevious(data.total_display_lines);
  useEffect(() => {
    if (wasTailing && data.total_display_lines !== prevTotalDisplayLines) {
      windowRef.current?.scrollToItem(data.total_display_lines - 1);
    }
  }, [data, wasTailing, prevTotalDisplayLines]);
  return (
    <main className={styles.main}>
      <nav className={styles.nav}>
        <Listbox as="div" className={styles.select} value={selectedLogSet} onChange={setSelectedLogSet}>
          <Listbox.Button className={styles['select-button']}>{selectedLogSet}</Listbox.Button>
          <Listbox.Options className={styles['select-options']}>
            {logSets.map((logSet) => (
              <Listbox.Option key={logSet} value={logSet}>{logSet}</Listbox.Option>
            ))}
          </Listbox.Options>
        </Listbox>
        <input ref={filterRef} className={styles.filter} type="text" placeholder="Filter logs by regex..."/>
        <button onClick={() => updateFilter()}>Update</button>
        <button onClick={() => clearFilter()}>Clear</button>
        <button
          hidden={isTailing}
          onClick={() => {
            if (windowRef.current) {
              windowRef.current.scrollToItem(data.total_display_lines - 1);
            }
          }}
        >Scroll to bottom</button>
      </nav>
      <div className={styles.logs}>
        <div ref={ref} className={styles.logsInner}>
          <div ref={ruler} className={styles.ruler}>0</div>
          <AutoSizer>
            {({width, height}) => (
              <FixedSizeList
                ref={windowRef}
                width={width}
                height={height}
                itemCount={data.total_display_lines}
                itemSize={charHeight ?? 0}
                overscanCount={10}
                onItemsRendered={({visibleStartIndex, visibleStopIndex}) => {
                  setVisibleStartIndex(visibleStartIndex);
                  setVisibleEndIndex(visibleStopIndex);
                }}
              >
                {({index, style}) => (
                  <div style={style}>
                    {(data.display_lines[index]?.spans || []).map((span, j) => (
                      <span
                        key={j}
                        className={
                          styles[`span-${span.label}${span.label == 'level' 
                            ? ('-' + data.display_lines[index]?.ll ?? 5) 
                            : ''
                          }`]}>
                        {span.text}
                      </span>
                    ))}
                  </div>
                )}
              </FixedSizeList>
            )}
          </AutoSizer>
        </div>
      </div>
      <div className={styles.status}>
        <div>{visibleStartIndex} – {visibleEndIndex} / {data.total_display_lines - 1} display lines</div>
        <div className={styles['status-scroll']}>
          <span hidden={isTailing}>SCROLLING</span>
          <span className="glowing" hidden={!isTailing}>TAILING</span>
          <div className={`dot ${isTailing ? 'glowing' : ''}`} hidden={!isTailing}></div>
        </div>
      </div>
    </main>
  );
}
