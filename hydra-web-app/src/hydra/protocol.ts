import { TaggedEnum } from "@/types/tagged-enum";

export const getHydraUrl = () => {
  if (process.env.NODE_ENV === "development") {
    return "http://localhost:8080/hydra";
  }

  return "https://d3be-173-3-124-82.ngrok-free.app/hydra";
};

export interface File {
  path: string;
  content: string;
}

export interface Request {
  files: File[];
}

export type Message = TaggedEnum<{
  PtyOutput: { output: string };
  PtyExit: never;
}>;

export async function sendRequest(req: Request) {
  const res = await fetch(`${getHydraUrl()}/execute`, {
    method: "POST",
    body: JSON.stringify(req),
    headers: {
      "Content-Type": "application/json",
    },
  });

  return await res.json();
}
