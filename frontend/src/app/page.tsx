"use client";

import {useDebounce, useMeasure} from "react-use";
import useWebSocket, { ReadyState } from 'react-use-websocket';
import styles from "./page.module.css";
import {useEffect, useRef, useState} from "react";

export default function Home() {
  const [socketUrl] = useState('ws://127.0.0.1:9000');
  const { sendMessage, lastMessage, readyState } = useWebSocket(socketUrl);
  const nextRequestId = useRef(0);
  const [ref, { width: lineWidth, height }] = useMeasure();
  const [ruler, { width: charWidth, height: charHeight }] = useMeasure();
  const [displayLines, setDisplayLines] = useState([]);
  useDebounce(() => {
    const cols = Math.floor(lineWidth / charWidth);
    const rows = Math.floor(height / charHeight);
    const requestId = nextRequestId.current++;
    if (cols > 0 && rows > 0) {
      sendMessage(JSON.stringify({
        id: requestId,
        method: "query",
        params: {
          log_file: "./var/test.log",
          cols,
          from: 0,
          to: rows
        }
      }));
    }
  }, 250, [charWidth, charHeight, lineWidth, height]);
  useEffect(() => {
    const data = lastMessage?.data;
    if (data) {
      const response = JSON.parse(data);
      const result = response["result"];
      setDisplayLines(result["display_lines"]);
    }
  }, [lastMessage]);
  return (
    <main className={styles.main}>
      <nav className={styles.nav}>
        <input type="text" placeholder="Filter logs by regex..."/>
        <button>Update</button>
        <button>Clear</button>
      </nav>
      <div ref={ref} className={styles.logs}>
        {displayLines.map((line: any) => (<div key={line.lln}>{line.text}</div>))}
      </div>
      <div ref={ruler} className={styles.ruler}>0</div>
    </main>
  );
}
