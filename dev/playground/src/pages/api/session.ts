import { getHydraUrl } from "@/hydra/protocol";
import { NextApiRequest, NextApiResponse } from "next";
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

const handler = async (req: NextApiRequest, res: NextApiResponse) => {
  const url = getHydraUrl();
  const requestUrl =
    url + "/execute?api_key=" + (process.env.HYDRA_API_KEY || "hydra");

  const body = requestSchema.safeParse(req.body);

  if (!body.success) {
    // return res.status(400).json({
    //   error: "Invalid request body",
    //   details: body.error,
    // });
    return res.status(400).json({
      error: "Invalid request body",
      details: body.error,
    });
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
    return res.status(500).json({
      error: "Hydra error",
      details: await hydraResponse.text(),
    });
  }

  const data: { ticket: string } = await hydraResponse.json();

  return res.json({ ticket: data.ticket, options });
};

export default handler;
