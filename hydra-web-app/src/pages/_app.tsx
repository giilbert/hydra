import { ChakraProvider } from "@chakra-ui/react";
import { AppProps } from "next/app";
import "xterm/css/xterm.css";
import "../global.css";

export default function App({ Component, pageProps }: AppProps) {
  return (
    <ChakraProvider>
      <Component {...pageProps} />
    </ChakraProvider>
  );
}
