FROM rust:1.31

WORKDIR /usr/src/rustysignal
COPY . .

RUN cargo install --path .

ENV http_proxy host:port
ENV https_proxy host:port

EXPOSE 3003

CMD ["rustysignal", "127.0.0.1:3003"]

