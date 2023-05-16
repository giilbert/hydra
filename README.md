# hydra
Sandboxed code execution. *with a terminal*

---

## Running

### Requirements:
- Docker (my version is 20.10.22)
- A Rust compiler (my version is 1.69.0)

### Steps:
#### Building `hydra-container`:
1. `cd` into `hydra-container`
2. Run `docker build -t hydra-container`.

This will create an image called `hydra-container`, containing the supervisor for running client code in the container sandbox.
The name must be `hydra-container`, since the name is hardcoded.

#### Building and running the server:
1. `cd` into `hydra-server`
2. Run `RUST_LOG=info cargo run --release`.

This'll run the server in release mode (you may leave it out) with logging level set to info.
The server, by default will listen to `0.0.0.0:3001`

#### Optional: Running the web (test) client
1. `cd` into `hydra-web-app`
2. `yarn`
3. `yarn dev`
