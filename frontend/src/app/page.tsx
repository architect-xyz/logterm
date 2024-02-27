"use client";

import {useCallback, useEffect, useRef, useState} from "react";
import {produce} from "immer";
import {useDebounce, useMeasure, usePrevious} from "@uidotdev/usehooks";
import useWebSocket, { ReadyState } from 'react-use-websocket';
import AutoSizer from "react-virtualized-auto-sizer";
import { FixedSizeList } from "react-window";
import styles from "./page.module.css";

type Logs = {
  total_display_lines: number,
  display_lines: DisplayLine[],
};

type LogsTail = {
  display_lines: DisplayLine[];
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

type Completion<T> = (value?: T | PromiseLike<T>) => void;

export default function Home() {
  // jsonrpc-over-websocket handling
  const [socketUrl] = useState('ws://127.0.0.1:9000');
  const { sendMessage, lastMessage, readyState } = useWebSocket(socketUrl);
  const nextRequestId = useRef(0);
  const inFlightRequests = useRef<{ [id: number]: Completion<void> }>({});
  const [data, setData] = useState<Logs>({
    total_display_lines: 0,
    display_lines: [],
  });
  useEffect(() => {
    const data = lastMessage?.data;
    if (data) {
      const response = JSON.parse(data);
      const id = response["id"] as number;
      if (inFlightRequests.current[id]) {
        inFlightRequests.current[id]();
        delete inFlightRequests.current[id];
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
  const reloadLog = useCallback((): Promise<void> => {
    if (cols) {
      const requestId = nextRequestId.current++;
      return new Promise((resolve) => {
        inFlightRequests.current[requestId] = resolve;
        sendMessage(JSON.stringify({
          id: requestId,
          method: "logs",
          params: {
            log_file: "./var/test.log",
            cols,
          }
        }));
      });
    }
    return Promise.resolve();
  }, [sendMessage, cols]);
  useEffect(() => {
    if (cols && rows) {
      reloadLog();
    }
  }, [reloadLog, cols, rows]);
  // react-window state
  const windowRef = useRef<FixedSizeList>(null);
  const [visibleStartIndex, setVisibleStartIndex] = useState(0);
  const [visibleEndIndex, setVisibleEndIndex] = useState(0);
  const isTailing = visibleEndIndex == data.total_display_lines - 1;
  const wasTailing = usePrevious(isTailing);
  useEffect(() => {
    if (wasTailing) {
      windowRef.current?.scrollToItem(data.total_display_lines - 1);
    }
  }, [data, isTailing]);
  return (
    <main className={styles.main}>
      <nav className={styles.nav}>
        <input type="text" placeholder="Filter logs by regex..."/>
        <button>Update</button>
        <button>Clear</button>
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
