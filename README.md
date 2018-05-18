## prometheus-cachet-glue

A simple lightweight daemon that takes in webhook calls from prometheus' alertmanager and
sets the state of a component according to that.

##### Limitations
Currently this expects the alerts array to be exactly one element.

### Running

You can either run it with docker:
```bash
docker run \
  -e CACHET_BASE_URL="https://your.cachet.instance" \
  -p 8888:88888 \
  matrixdotorg/prometheus-cachet-glue
```
or by compiling it yourself and running it on your host directly:
```bash
CACHET_BASE_URL="https://your.cachet.instance" prometheus-cachet-glue
```

As seen in the commands above, you can specify your cachet instance by setting the `CACHET_BASE_URL`
environment variable. Please note that this is not supposed to end with a slash.

The daemon is listening for http requests on port 8888.
You should use a reverse proxy for TLS termination in front of that.

### Building

You need to have a local rust toolchain installed. 
To get one, go to [rustup.rs](https://rustup.rs).

To build it, clone the repository and run `cargo install` in it:
```bash
git clone https://github.com/matrixdotorg/prometheus-cachet-glue.git pcg
cd pcg
cargo install
```

This will take some time. Cargo installs the compiled binary into ~/.cargo/bin, 
which *should* be in your PATH.

If you want to build the docker image instead, clone the repository and run `docker build`
in it:
```bash
git clone https://github.com/matrix-org/prometheus-cachet-glue.git pcg
docker build -t matrixdotorg/prometheus-cachet-glue pcg
```