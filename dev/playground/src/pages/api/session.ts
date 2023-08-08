import { getHydraUrl } from "@/hydra/protocol";
import { NextRequest, NextResponse } from "next/server";
import { z } from "zod";

const fileSchema = z.object({
  path: z.string(),
  content: z.string(),
});

const requestSchema = z
  .object({
    files: z.array(fileSchema),
  })
  .strict();

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
  const requestUrl =
    url + "/execute?api_key=" + (process.env.HYDRA_API_KEY || "hydra");

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

  const options = {
    ...body.data,
    persistent: false,
  };

  const hydraResponse = await fetch(requestUrl, {
    method: "POST",
    body: JSON.stringify(options),
    headers: {
      "Content-Type": "application/json",
    },
  });

  if (!hydraResponse.ok) {
    return NextResponse.json(
      {
        error: "Hydra error",
        details: await hydraResponse.text(),
      },
      { status: 500 }
    );
  }

  const data: { ticket: string } = await hydraResponse.json();

  return NextResponse.json({ ticket: data.ticket, options });
};

export default handler;
