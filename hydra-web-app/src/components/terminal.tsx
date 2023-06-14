import { useHydra } from "@/hydra/provider";
import { Box } from "@chakra-ui/react";
import { createRef, useEffect, useRef } from "react";
import type { IDisposable, Terminal as XTerm } from "xterm";
import type { FitAddon } from "xterm-addon-fit";

export const Terminal: React.FC = () => {
  const { events, sendInput } = useHydra();
  const containerRef = createRef<HTMLDivElement>();
  const terminalRef = useRef<XTerm>();
  const fitAddonRef = useRef<FitAddon>();
  const hasLoaded = useRef(false);
  const onDataHandler = useRef<IDisposable>();

  useEffect(() => {
    const onOutput = (output: string) => {
      if (terminalRef.current) {
        terminalRef.current.write(output);
      } else {
        console.warn("terminalRef.current is null while trying to write data");
      }
    };

    const onClear = () => {
      if (terminalRef.current) {
        terminalRef.current.clear();
      } else {
        console.warn("terminalRef.current is null while trying to clear");
      }
    };

    events.on("terminal:output", onOutput);
    events.on("terminal:clear", onClear);

    return () => {
      events.off("terminal:output", onOutput);
      events.off("terminal:clear", onClear);
    };
  }, [events]);

  useEffect(() => {
    async function init() {
      const { Terminal } = await import("xterm");
      const { FitAddon } = await import("xterm-addon-fit");

      if (!containerRef.current) return;
      hasLoaded.current = true;

      const terminal = new Terminal();
      const fitAddon = new FitAddon();
      terminal.loadAddon(fitAddon);
      terminal.open(containerRef.current);
      fitAddon.fit();

      terminalRef.current = terminal;
      fitAddonRef.current = fitAddon;

      onDataHandler.current = terminal.onData((data) => sendInput(data));
    }

    if (!hasLoaded.current && containerRef.current) init();
  }, [containerRef, sendInput]);

  useEffect(() => {
    return () => {
      if (terminalRef.current) terminalRef.current.dispose();
      if (onDataHandler.current) onDataHandler.current.dispose();
      if (fitAddonRef.current) fitAddonRef.current.dispose();
      hasLoaded.current = false;
    };
  }, []);

  return <Box ref={containerRef} p="2" bg="black" h="full" />;
};
