import { TaggedEnum } from "@/types/tagged-enum";

export const getHydraUrl = () => {
  if (process.env.NODE_ENV === "development") {
    return "http://0.0.0.0:3001";
  }

  if (
    process.env.NODE_ENV === "production" &&
    process.env.NEXT_PUBLIC_HYDRA_URL
  ) {
    return process.env.NEXT_PUBLIC_HYDRA_URL as string;
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

export async function createRunRequest(req: Request) {
  const res = await fetch("/api/run", {
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
