import { useToast } from "@chakra-ui/react";
import {
  createContext,
  useCallback,
  useContext,
  useRef,
  useState,
} from "react";
import { Emitter } from "strict-event-emitter";
import { File, getHydraUrl, Message, createRunRequest } from "./protocol";

type HydraState = "idle" | "loading" | "running" | "error";

interface HydraContextValue {
  status: HydraState;
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
  const eventsRef = useRef<Emitter<HydraEvents>>(new Emitter());
  const toast = useToast();

  const run = useCallback(
    async (files: File[]) => {
      eventsRef.current.emit("terminal:clear");
      if (ws.current?.readyState === WebSocket.OPEN) ws.current.close();

      setStatus("loading");

      let data: { ticket: string } = undefined as any;
      try {
        data = await createRunRequest({
          files,
        });
      } catch (e) {
        console.error(e);
        toast({
          title: "Error",
          description: "Failed to create run request. More info in console.",
          status: "error",
          isClosable: true,
        });
        setStatus("error");
        return;
      }

      ws.current = new WebSocket(
        `${getHydraUrl().replace("http", "ws")}/execute?ticket=` + data.ticket
      );

      ws.current.addEventListener("open", () => {
        if (!ws.current) throw new Error("ws.current is null");
        toast({
          title: "Web Gateway URL",
          description: `${data.ticket}--[PORT].localhost:3101`,
        });
        ws.current.send(
          JSON.stringify({
            type: "Run",
            data: null,
          })
        );
        setStatus("running");
      });

      ws.current.addEventListener("message", (event) => {
        const { type, data }: Message = JSON.parse(event.data);
        if (type === "PtyOutput") {
          eventsRef.current.emit("terminal:output", data.output);
        }
        if (type === "PtyExit") {
          ws.current?.close(1000);
          setStatus("idle");
        }
      });

      ws.current.addEventListener("close", (e) => {
        if (!e.wasClean) {
          console.error(
            `[${data.ticket.slice(0, 5)}] Not a clean close | Code: ${e.code}`
          );
          console.error(e);
          setStatus("error");
        } else {
          console.log(
            `[${data.ticket.slice(0, 5)}] Clean close | Code: ${e.code}`
          );
          setStatus("idle");
        }
      });
    },
    [toast]
  );

  const crash = useCallback(() => {
    if (!ws.current) return;
    ws.current.send(
      JSON.stringify({
        type: "Crash",
        data: null,
      })
    );
  }, []);

  const sendInput = useCallback((input: string) => {
    if (!ws.current) return;
    ws.current.send(
      JSON.stringify({
        type: "PtyInput",
        data: { id: 0, input },
      })
    );
  }, []);

  return (
    <HydraContext.Provider
      value={{
        status,
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
