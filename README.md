# catboost-server

CatBoost server in Rust + gRPC

## Model

Simple CatBoost model

* 2 numeric (float) features
* 3 categorical features

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
% latency : ms / req
% warm up with 10 times

| batch size | requests | mean | p95 | p99 | p99.9 | max | timeouts |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| 32 | 100k | 0.217 ms | 0.291 ms | 0.408 ms | 0.533 ms | 0.803 ms | 0 |
| 64 | 100k | 0.299 ms | 0.409 ms | 0.537 ms | 0.786 ms | 4.672 ms | 0 |
| 128 | 100k | 0.461 ms | 0.630 ms | 0.782 ms | 0.990 ms | 1.696 ms | 0 |
| 256 | 100k | | | | | | |

## Reference

* https://github.com/leosmerling/catboost_inference/tree/main/rust/grpc-server
