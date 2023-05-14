import { NextPage } from "next";
import { createRef, useCallback, useEffect, useRef, useState } from "react";
import type { Terminal } from "xterm";

async function sendRequest() {
  const res = await fetch("http://localhost:3001/execute", {
    method: "POST",
    body: JSON.stringify({
      files: [
        {
          path: "main.py",
          content: 'print("Hello " + input("What is your name? ") + "!")',
        },
      ],
    }),
    headers: {
      "Content-Type": "application/json",
    },
  });

  return await res.json();
}

const IndexPage: NextPage = () => {
  const terminalContainerRef = createRef<HTMLDivElement>();
  const [isLoading, setIsLoading] = useState(false);
  const [running, setRunning] = useState(false);
  const isLoadingXTerm = useRef(false);
  const terminal = useRef<Terminal>();
  const ws = useRef<WebSocket>();

  useEffect(() => {
    async function load() {
      if (
        isLoadingXTerm.current ||
        terminal.current ||
        !terminalContainerRef.current
      )
        return;
      const { Terminal } = await import("xterm");
      terminal.current = new Terminal();
      terminal.current.open(terminalContainerRef.current);

      terminal.current.onData((data) => {
        ws.current?.send(
          JSON.stringify({
            type: "PtyInput",
            data: {
              id: 0,
              input: data,
            },
          })
        );
      });
    }

    if (terminal.current || !terminalContainerRef.current) return;
    load();
    isLoadingXTerm.current = true;
  }, [terminalContainerRef]);

  const run = useCallback(async () => {
    terminal.current?.clear();
    if (ws.current?.readyState === WebSocket.OPEN) ws.current.close();

    setIsLoading(true);

    const data = await sendRequest();

    ws.current = new WebSocket(
      "ws://localhost:3001/execute?ticket=" + data.ticket
    );

    ws.current.addEventListener("open", () => {
      ws.current!.send(
        JSON.stringify({
          type: "Run",
          data: null,
        })
      );
      setIsLoading(false);
      setRunning(true);
    });

    ws.current.addEventListener("message", (event) => {
      const { type, data } = JSON.parse(event.data);
      if (type === "PtyOutput") terminal.current?.write(data.output);
      if (type === "PtyExit") {
        ws.current?.close();
        setRunning(false);
      }
    });
  }, []);

  const crash = useCallback(() => {
    ws.current!.send(
      JSON.stringify({
        type: "Crash",
        data: null,
      })
    );
  }, []);

  return (
    <div>
      <button onClick={run}>run</button>
      <button onClick={crash}>crash</button>
      {isLoading && <p>Loading</p>}
      {running ? <p>Running</p> : <p>Not running</p>}
      <div ref={terminalContainerRef}></div>
    </div>
  );
};

export default IndexPage;
