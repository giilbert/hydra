import { getHydraUrl } from "@/hydra/protocol";
import { NextApiHandler, NextApiRequest, NextApiResponse } from "next";
import { NextRequest, NextResponse } from "next/server";
import { z } from "zod";

const fileSchema = z.object({
  path: z.string(),
  content: z.string(),
});

const requestSchema = z.object({
  files: z.array(fileSchema),
});

// const handler: NextApiHandler = async (
//   req: NextApiRequest,
//   res: NextApiResponse
// ) => {
//   const url = getHydraUrl();
//   const requestUrl = url + "/execute?api_key=" + process.env.HYDRA_API_KEY;

//   const body = requestSchema.safeParse(req.body);

//   if (!body.success) {
//     return res.status(400).json({
//       error: "Invalid request body",
//       details: body.error,
//     });
//   }

//   const hydraResponse = await fetch(requestUrl, {
//     method: "POST",
//     body: JSON.stringify(body.data),
//     headers: {
//       "Content-Type": "application/json",
//     },
//   });

//   const data: { ticket: string } = await hydraResponse.json();

//   return res.json(data);
// };

export const config = {
  runtime: "edge",
};

const handler = async (req: NextRequest) => {
  const url = getHydraUrl();
  const requestUrl = url + "/execute?api_key=" + process.env.HYDRA_API_KEY;

  const body = requestSchema.safeParse(await req.json());

  if (!body.success) {
    // return res.status(400).json({
    //   error: "Invalid request body",
    //   details: body.error,
    // });
    return NextResponse.json(
      {
        error: "Invalid request body",
        details: body.error,
      },
      { status: 400 }
    );
  }

  const hydraResponse = await fetch(requestUrl, {
    method: "POST",
    body: JSON.stringify(body.data),
    headers: {
      "Content-Type": "application/json",
    },
  });

  const data: { ticket: string } = await hydraResponse.json();

  return NextResponse.json({ ticket: data.ticket });
};

export default handler;
