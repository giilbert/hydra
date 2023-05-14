import { AppProps } from "next/app";
import "xterm/css/xterm.css";

export default function App({ Component, pageProps }: AppProps) {
  return <Component {...pageProps} />;
}
