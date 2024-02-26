"use client";

import {useDebounce, useMeasure} from "@uidotdev/usehooks";
import useWebSocket, { ReadyState } from 'react-use-websocket';
import AutoSizer from "react-virtualized-auto-sizer";
import { FixedSizeList } from "react-window";
import styles from "./page.module.css";
import {useCallback, useEffect, useRef, useState} from "react";

type QueryResponse = {
  total_display_lines: number;
  display_lines: DisplayLine[];
  row_offset: number;
};

type DisplayLine = {
  lln: number,
  ts?: Date | null,
  level: number,
  text: string,
  matches?: [number, number][] | null,
};

type Completion<T> = (value?: T | PromiseLike<T>) => void;

export default function Home() {
  // jsonrpc-over-websocket handling
  const [socketUrl] = useState('ws://127.0.0.1:9000');
  const { sendMessage, lastMessage, readyState } = useWebSocket(socketUrl);
  const nextRequestId = useRef(0);
  const inFlightRequests = useRef<{ [id: number]: Completion<void> }>({});
  const [data, setData] = useState<QueryResponse>({
    total_display_lines: 0,
    display_lines: [],
    row_offset: 0
  });
  useEffect(() => {
    const data = lastMessage?.data;
    if (data) {
      const response = JSON.parse(data);
      const id = response["id"] as number;
      const result = response["result"] as QueryResponse;
      if (inFlightRequests.current[id]) {
        inFlightRequests.current[id]();
        delete inFlightRequests.current[id];
        setData(result);
      } else {
        console.warn("unexpected response:", id);
      }
    }
  }, [inFlightRequests, lastMessage]);
  // display measurement and react-window callbacks
  const [ref, { width: lineWidth, height }] = useMeasure();
  const [ruler, { width: charWidth, height: charHeight }] = useMeasure();
  const cols = useDebounce(lineWidth && charWidth && Math.floor(lineWidth / charWidth), 300);
  const rows = useDebounce(height && charHeight && Math.floor(height / charHeight), 300);
  const reloadLog = useCallback((): Promise<void> => {
    if (cols) {
      const requestId = nextRequestId.current++;
      return new Promise((resolve) => {
        inFlightRequests.current[requestId] = resolve;
        sendMessage(JSON.stringify({
          id: requestId,
          method: "query",
          params: {
            log_file: "./var/test.log",
            cols,
            from: 0,
          }
        }));
      });
    }
    return Promise.resolve();
  }, [sendMessage, cols]);
  // reload logs on resize
  useEffect(() => {
    if (cols && rows) {
      reloadLog();
    }
  }, [reloadLog, cols, rows]);
  return (
    <main className={styles.main}>
      <nav className={styles.nav}>
        <input type="text" placeholder="Filter logs by regex..."/>
        <button>Update</button>
        <button>Clear</button>
      </nav>
      <div ref={ruler} className={styles.ruler}>0</div>
      <div ref={ref} className={styles.logs}>
        <AutoSizer>
          {({ width, height }) => (
            <FixedSizeList
              width={width}
              height={height}
              itemCount={data.total_display_lines}
              itemSize={charHeight ?? 0}
            >
              {({ index, style }) => (
                <div style={style}>
                  {data.display_lines[index]?.text || "#not found"}
                </div>
              )}
            </FixedSizeList>
          )}
        </AutoSizer>
      </div>
    </main>
  );
}
