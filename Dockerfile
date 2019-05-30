FROM rust:1.31

WORKDIR /usr/src/rustysignal
COPY . .

RUN cargo install --path .

ENV http_proxy 0.0.0.0:3003
ENV https_proxy 0.0.0.0:3003

EXPOSE 3003

CMD ["rustysignal", "0.0.0.0:3003"]

