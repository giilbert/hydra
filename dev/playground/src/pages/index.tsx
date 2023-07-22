import { Editor } from "@/components/editor";
import { Terminal } from "@/components/terminal";
import { HydraProvider } from "@/hydra/provider";
import { Button, Grid, GridItem, useColorMode } from "@chakra-ui/react";
import { NextPage } from "next";
import Head from "next/head";

const IndexPage: NextPage = () => {
  const { toggleColorMode } = useColorMode();

  return (
    <HydraProvider>
      <Head>
        <title>Hydra</title>
        <meta
          name="description"
          content="Code execution with a terminal doododooooo"
        />
      </Head>

      <Button position="fixed" bottom="4" left="4" onClick={toggleColorMode}>
        Toggle Color Mode
      </Button>
      <Grid p="4" gridTemplateColumns="1fr 1fr" h="100vh" gap="4">
        <GridItem>
          <Editor />
        </GridItem>
        <GridItem h="full">
          <Terminal />
        </GridItem>
      </Grid>
    </HydraProvider>
  );
};

export default IndexPage;
