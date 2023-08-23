import { TaggedEnum } from "@/types/tagged-enum";

export const getHydraUrl = () => {
  if (process.env.NODE_ENV === "development") {
    return "http://0.0.0.0:3100";
  }

  if (process.env.NODE_ENV === "production" && process.env.HYDRA_SERVER_URL) {
    return process.env.HYDRA_SERVER_URL as string;
  }

  throw new Error("Cannot determine HYDRA_URL");
};

// for client side websocket + proxy requests
export const getHydraProxyUrl = () => {
  if (process.env.NODE_ENV === "development") {
    return "http://localhost:3101";
  }

  if (
    process.env.NODE_ENV === "production" &&
    process.env.NEXT_PUBLIC_HYDRA_PROXY_URL
  ) {
    return process.env.NEXT_PUBLIC_HYDRA_PROXY_URL as string;
  }

  throw new Error("Cannot determine HYDRA_PROXY_URL");
};

export interface File {
  path: string;
  content: string;
}

export interface Request {
  files: File[];
}

export type ClientSent = TaggedEnum<{
  Ping: undefined;
  Run: undefined;
  PtyInput: { id: number; input: string };
  Crash: undefined;
}>;

export type ServerSent = TaggedEnum<{
  PtyOutput: { output: string };
  PtyExit: never;
}>;

export async function createRunRequest(req: Request) {
  const res = await fetch("/api/session", {
    method: "POST",
    body: JSON.stringify(req),
    headers: {
      "Content-Type": "application/json",
    },
  });

  if (!res.ok) {
    throw new Error("Failed to create run request");
  }

  const data: { ticket: string } = await res.json();

  return data;
}
