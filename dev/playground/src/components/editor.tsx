import { File } from "@/hydra/protocol";
import { useHydra } from "@/hydra/provider";
import {
  Badge,
  Box,
  Button,
  Divider,
  HStack,
  Input,
  Text,
  useColorMode,
  useColorModeValue,
} from "@chakra-ui/react";
import { python } from "@codemirror/lang-python";
import ReactCodeMirror from "@uiw/react-codemirror";
import Link from "next/link";
import { useCallback, useState } from "react";

export const Editor: React.FC = () => {
  const { run, crash, status, ticket } = useHydra();
  // const [files, setFiles] = useState<File[]>([
  //   {
  //     path: "main.py",
  //     content: `print("Hello world!")`,
  //   },
  // ]);
  const [files, setFiles] = useState<File[]>([
    {
      path: "main.py",
      content: `from turtle import Turtle
from time import sleep
from random import random

sleep(1)

t = Turtle()

while True:
  t.forward(100)
  t.left(179)
  # change the pen color to a random color
  t.color(random(), random(), random())
  `,
    },
  ]);
  const [selectedFile, setSelectedFile] = useState<string>("main.py");
  const [input, setInput] = useState<string>("");
  const { colorMode } = useColorMode();
  const fadedTextColor = useColorModeValue("blackAlpha.700", "whiteAlpha.700");

  const addFile = useCallback((name: string) => {
    setFiles((files) => [
      ...files,
      {
        path: name,
        content: "",
      },
    ]);
  }, []);

  const removeFile = useCallback((name: string) => {
    setFiles((files) => files.filter((file) => file.path !== name));
  }, []);

  const onChange = useCallback(
    (content: string) => {
      setFiles((files) => [
        ...files.filter((file) => file.path !== selectedFile),
        {
          path: selectedFile,
          content,
        },
      ]);
    },
    [selectedFile]
  );

  const onRun = useCallback(() => {
    run(files);
  }, [run, files]);

  return (
    <Box>
      <HStack>
        <Button
          onClick={onRun}
          isLoading={status === "loading"}
          colorScheme="green"
        >
          Run
        </Button>
        {status === "running" && (
          <>
            <Button onClick={crash} colorScheme="red" mr="2">
              Crash
            </Button>
            <Link
              href={`http://${ticket}--6080.localhost:3101`}
              target="_blank"
              rel="noopener noreferrer"
            >
              Proxy Link (port 6080)
            </Link>
            <Link
              href={`https://${ticket}--6080.astrius-ingress.techcodes.org`}
              target="_blank"
              rel="noopener noreferrer"
            >
              Proxy Link (port 6080) (on test)
            </Link>
          </>
        )}

        <Box my="2" ml="auto">
          {status === "idle" && <Badge fontSize="1rem">Idle</Badge>}
          {status === "loading" && (
            <Badge fontSize="1rem" colorScheme="yellow">
              Loading
            </Badge>
          )}
          {status === "running" && (
            <Badge fontSize="1rem" colorScheme="green">
              Running
            </Badge>
          )}
          {status === "error" && (
            <Badge fontSize="1rem" colorScheme="red">
              Error
            </Badge>
          )}
        </Box>
      </HStack>

      <Divider my="2" />

      <Text mb="2" color={fadedTextColor}>
        Files
      </Text>

      <HStack my="2">
        {files.map((file) => (
          <HStack
            key={file.path}
            bg="whiteAlpha.200"
            px="3"
            py="1"
            borderRadius="md"
            cursor="pointer"
            onClick={() => setSelectedFile(file.path)}
          >
            <Box>{file.path}</Box>
            <Text
              color={fadedTextColor}
              _hover={{ textDecoration: "underline" }}
              onClick={(e) => {
                e.stopPropagation();
                removeFile(file.path);
              }}
            >
              Delete
            </Text>
          </HStack>
        ))}
      </HStack>

      <HStack my="2">
        <Input
          size="sm"
          placeholder="File name"
          value={input}
          onChange={(e) => {
            setInput(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              if (input === "") return;
              addFile(input);
              setInput("");
            }
          }}
        />
        <Button
          size="sm"
          onClick={() => {
            if (input === "") return;
            addFile(input);
            setInput("");
          }}
          w="24"
        >
          Add file
        </Button>
      </HStack>

      {files.findIndex((file) => file.path === selectedFile) !== -1 && (
        <>
          <Divider my="2" />
          <Text mb="2" color={fadedTextColor}>
            Editing: {selectedFile}
          </Text>
          <ReactCodeMirror
            value={
              files.find((file) => file.path === selectedFile)?.content || ""
            }
            onChange={onChange}
            extensions={[python()]}
            theme={colorMode}
            height="300px"
          />
        </>
      )}
    </Box>
  );
};
