import { TaggedEnum } from "@/types/tagged-enum";

export const getHydraUrl = () => {
  if (process.env.NODE_ENV === "development") {
    return "http://localhost:8080/hydra";
  }

  if (process.env.NODE_ENV === "production") {
    return process.env.HYDRA_URL;
  }

  throw new Error("Cannot determine HYDRA_URL");
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
