import { useToast } from "@chakra-ui/react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { Emitter } from "strict-event-emitter";
import {
  File,
  getHydraUrl,
  ClientSent,
  createRunRequest,
  ServerSent,
} from "./protocol";

type HydraState = "idle" | "loading" | "running" | "error" | "connected";

interface HydraContextValue {
  status: HydraState;
  ticket?: string;
  run: (files: File[]) => void;
  crash: () => void;
  sendInput: (input: string) => void;
  events: Emitter<HydraEvents>;
}

type HydraEvents = {
  "terminal:output": [string];
  "terminal:clear": [];
};

const HydraContext = createContext<HydraContextValue>({
  status: "idle",
  ticket: undefined,
  run: () => {},
  crash: () => {},
  sendInput: () => {},
  events: new Emitter(),
});

export const useHydra = () => {
  const context = useContext(HydraContext);
  if (!context) throw new Error("useHydra must be used within a HydraProvider");
  return context;
};

export const HydraProvider: React.FC<{
  children: React.ReactNode;
}> = ({ children }) => {
  const [status, setStatus] = useState<HydraState>("idle");
  const ws = useRef<WebSocket>();
  const ticket = useRef<string>();
  const eventsRef = useRef<Emitter<HydraEvents>>(new Emitter());
  const disposeRef = useRef<() => void>();
  const toast = useToast();
  const isPersistent = false;

  const connect = useCallback(
    async (files: File[]) => {
      setStatus("loading");
      console.log("connecting...");

      let data: { ticket: string } = undefined as any;
      try {
        data = await createRunRequest({
          files,
        });
      } catch (e) {
        console.error(e);
        toast({
          title: "Error",
          description: "Failed to create session. More info in console.",
          status: "error",
          isClosable: true,
        });
        setStatus("error");
        return;
      }

      ticket.current = data.ticket;
      ws.current = new WebSocket(
        `${getHydraUrl().replace("http", "ws")}/execute?ticket=` +
          ticket.current
      );

      const onMessage = (event: MessageEvent) => {
        const { type, data }: ServerSent = JSON.parse(event.data);
        console.log("in", type, data);

        if (type === "PtyOutput") {
          eventsRef.current.emit("terminal:output", data.output);
        }

        if (type === "PtyExit" && !isPersistent) {
          setStatus("idle");
        }
      };

      const onClose = (_e: CloseEvent) => {
        console.log(`[${ticket.current?.slice(0, 5)}] Connection closed`);
        setStatus("idle");
      };

      ws.current.addEventListener("message", onMessage);
      ws.current.addEventListener("close", onClose);

      await new Promise((resolve) => {
        const onOpen = () => {
          resolve(undefined);
          ws.current?.removeEventListener("open", onOpen);
        };
        ws.current?.addEventListener("open", onOpen);
      });

      const interval = setInterval(() => {
        if (ws.current)
          ws.current.send(
            JSON.stringify({
              type: "Ping",
              data: undefined,
            } satisfies ClientSent)
          );
      }, 1000);

      return () => {
        console.log("disposing");
        clearInterval(interval);
        ws.current?.removeEventListener("message", onMessage);
        ws.current?.removeEventListener("close", onClose);
      };
    },
    [toast, isPersistent]
  );

  const sendMessage = useCallback((message: ClientSent) => {
    if (!ws.current) throw new Error("ws.current is null");
    console.log("out", message.type, message.data);
    ws.current.send(JSON.stringify(message));
  }, []);

  const run = useCallback(
    async (files: File[]) => {
      eventsRef.current.emit("terminal:clear");

      if (status !== "running" && status !== "connected") {
        const dispose = await connect(files);
        if (disposeRef.current) {
          disposeRef.current();
        }
        disposeRef.current = dispose;
      }
      if (!ws.current) return;
      if (!ticket.current) return;

      sendMessage({
        type: "Run",
        data: undefined,
      });
      setStatus("running");
    },
    [connect, status, sendMessage]
  );

  const crash = useCallback(() => {
    sendMessage({
      type: "Crash",
      data: undefined,
    });
  }, [sendMessage]);

  const sendInput = useCallback(
    (input: string) => {
      sendMessage({
        type: "PtyInput",
        data: { id: 0, input },
      });
    },
    [sendMessage]
  );

  useEffect(() => {
    return () => {
      if (disposeRef.current) disposeRef.current();
    };
  }, []);

  return (
    <HydraContext.Provider
      value={{
        status,
        ticket: ticket.current,
        run,
        crash,
        sendInput,
        events: eventsRef.current,
      }}
    >
      {children}
    </HydraContext.Provider>
  );
};
