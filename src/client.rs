use std::error::Error;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use std::{env, process};
use tokio;

pub mod cb {
    tonic::include_proto!("cb");
}

const REPORT: usize = 100000;

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[derive(Debug, Clone)]
struct Config {
    addr: String,
    users: usize,
    b: usize,
    n: usize,
    timeout: u64,
}

impl Config {
    fn new(args: &[String]) -> Result<Config, &'static str> {
        if args.len() < 5 {
            return Err("not enough arguments");
        }

        let addr = args[1].clone();
        let users = args[2].parse().unwrap();
        let b = args[3].parse().unwrap();
        let n = args[4].parse().unwrap();
        let timeout = args[5].parse().unwrap();

        Ok(Config {
            addr: addr,
            users: users,
            b: b,
            n: n,
            timeout: timeout,
        })
    }
}

#[derive(Default, Debug)]
struct Metrics {
    lat: Vec<u64>,
    model_lat: Vec<u64>,
    total_secs: f32,
}

#[derive(Default)]
struct User {
    id: String,
}

impl User {
    fn new(id: String) -> Self {
        Self { id }
    }

    async fn execute(&mut self, config: &Config) -> Result<Metrics, Box<dyn Error>> {
        let mut client =
            cb::inference_client::InferenceClient::connect(config.addr.clone()).await?;

        let request = cb::PredictRequest {
            features: vec![
                cb::Features {
                    float_feature1: 0.55,
                    float_feature2: 0.33,
                    cat_feature1: "A".to_string(),
                    cat_feature2: "B".to_string(),
                    cat_feature3: "C".to_string(),
                };
                config.b
            ],
        };

        // warm up 10 times
        for _ in 0..10 {
            _ = client.predict(request.clone()).await?;
        }

        let mut report_start = Instant::now();
        let mut total_secs = 0.0f32;
        let mut lat = vec![0u64; config.n];
        let mut model_lat = vec![0u64; config.n];

        for i in 1..config.n {
            let start = Instant::now();
            let response = client.predict(request.clone()).await?.into_inner();

            lat[i] = start.elapsed().as_nanos() as u64;
            model_lat[i] = response.model_latency;

            if i % REPORT == 0 {
                let secs = report_start.elapsed().as_secs_f32();

                println!("--------------------------------------------------------------------------------------------------------------------------------------------------------");
                log_stats(
                    ">Model",
                    i,
                    config.n,
                    secs,
                    config.timeout,
                    &model_lat,
                    i - REPORT,
                    REPORT,
                );
                log_stats(
                    &self.id,
                    i,
                    config.n,
                    secs,
                    config.timeout,
                    &lat,
                    i - REPORT,
                    REPORT,
                );
                println!("--------------------------------------------------------------------------------------------------------------------------------------------------------");

                total_secs += secs;
                report_start = Instant::now();
            }
        }

        Ok(Metrics {
            lat,
            model_lat,
            total_secs,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("========================================================================================================================================================");
    println!("Usage: cb-client http://host:port users batch_size iterations timeout_ms");

    let args: Vec<String> = env::args().collect();

    let config = Config::new(&args).unwrap_or_else(|err| {
        println!("Problem parsing arguments: {}", err);
        process::exit(1);
    });

    println!(
        "Host:{} Users:{} BatchSize:{} Iterations:{} Timeout:{}",
        config.addr, config.users, config.b, config.n, config.timeout
    );
    println!("========================================================================================================================================================");

    let (tx, rx) = mpsc::channel::<Metrics>();
    let start = Instant::now();

    for u in 0..config.users {
        let config = config.clone();
        let tx = tx.clone();
        let id = format!("User{:0>2}", u);

        tokio::spawn(async move {
            let metrics = User::new(id.clone())
                .execute(&config)
                .await
                .expect("ERROR!");
            report(&id, 1, &config, &metrics);
            tx.send(metrics).unwrap();
        });
    }

    let mut metrics = Metrics::default();
    for _ in 0..config.users {
        let result = rx.recv().unwrap();

        metrics.lat.extend(result.lat.iter());
        metrics.model_lat.extend(result.model_lat.iter());
        metrics.total_secs = start.elapsed().as_secs_f32();
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
    report("TEST  ", config.users, &config, &metrics);

    Ok(())
}

fn log_stats(
    title: &str,
    i: usize,
    n: usize,
    secs: f32,
    timeout: u64,
    latencies: &Vec<u64>,
    skip: usize,
    take: usize,
) {
    let mean = latencies.iter().skip(skip).take(take).sum::<u64>() / (i - skip) as u64;
    let max = *latencies.iter().skip(skip).take(take).max().unwrap();
    let count = latencies
        .iter()
        .skip(skip)
        .take(take)
        .collect::<Vec<&u64>>()
        .len();
    let timeouts = latencies
        .iter()
        .skip(skip)
        .take(take)
        .filter(|x| Duration::from_nanos(**x as u64) > Duration::from_millis(timeout))
        .collect::<Vec<&u64>>()
        .len();
    let success_ratio = 100.0 - 100.0 * (timeouts as f32 / n as f32);

    let ps: Vec<String> = percentiles(vec![0.95, 0.99, 0.999], latencies, skip, take)
        .iter()
        .map(|(p, x)| format!("p{:2.1}={:1.3}ms", 100.0 * p, *x as f64 * 1e-6))
        .collect();

    println!(
        "{}: {} Mean={:1.3}ms Max={:1.3}m Count={:>7} Req/s={:0>4.0} Timeouts={:0>3} Succ={:0>3.3}% {}",
        title,
        i,
        mean as f64 * 1e-6,
        max as f64 * 1e-6,
        count,
        count as f32 / secs,
        timeouts,
        success_ratio,
        ps.join(" ")
    );
}

fn percentiles(ps: Vec<f64>, latencies: &Vec<u64>, skip: usize, take: usize) -> Vec<(f64, u64)> {
    let mut sorted: Vec<&u64> = latencies.iter().skip(skip).take(take).collect();
    sorted.sort();

    ps.iter()
        .map(|p| (*p, *sorted[(sorted.len() as f64 * p) as usize]))
        .collect()
}

fn report(id: &str, users: usize, config: &Config, metrics: &Metrics) {
    println!("REPORT =================================================================================================================================================");
    log_stats(
        ">Model",
        users * config.n,
        users * config.n,
        metrics.total_secs,
        config.timeout,
        &metrics.model_lat,
        0,
        users * config.n,
    );
    log_stats(
        &id,
        users * config.n,
        users * config.n,
        metrics.total_secs,
        config.timeout,
        &metrics.lat,
        0,
        users * config.n,
    );
    println!("========================================================================================================================================================");
}
