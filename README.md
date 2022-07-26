# catboost-server

CatBoost server in Rust + gRPC

## Run

### gRPC Server

```shell
$ cargo run --release --bin cb-server 
```

### gRPC Client

```shell
# binary http://host:port num_users batch_size iterations timeout
$ cargo run --release --bin cb-client http://127.0.0.1:50051 1 32 100000 10
```

## Performance

% timeout : 10ms

| batch size | requests | mean | p95 | p99 | p99.9 | max | timeouts |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| 32 | 100k | 0.217 ms | 0.291 ms | 0.408 ms | 0.533 ms | 0.803 ms | 0 |
| 64 | 100k | | | | | | |
| 128 | 100k | | | | | | |
| 256 | 100k | | | | | | |

## Reference

* https://github.com/leosmerling/catboost_inference/tree/main/rust/grpc-server
