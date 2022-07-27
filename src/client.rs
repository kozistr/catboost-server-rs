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
    b: usize,
    n: usize,
    timeout: u64,
}

impl Config {
    fn new(args: &[String]) -> Result<Config, &'static str> {
        if args.len() < 4 {
            return Err("not enough arguments");
        }

        let addr = args[1].clone();
        let b = args[2].parse().unwrap();
        let n = args[3].parse().unwrap();
        let timeout = args[4].parse().unwrap();

        Ok(Config {
            addr: addr,
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

async fn execute(config: &Config) -> Result<Metrics, Box<dyn Error>> {
    let mut client = cb::inference_client::InferenceClient::connect(config.addr.clone()).await?;

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

            println!("----------------------------------------------------------------------------------------------------------------------------------------------");
            log_stats(
                "> Model",
                i,
                config.n,
                secs,
                config.timeout,
                &model_lat,
                i - REPORT,
                REPORT,
            );
            println!("----------------------------------------------------------------------------------------------------------------------------------------------");

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

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("==============================================================================================================================================");
    println!("Usage: cb-client http://host:port batch_size iterations timeout_ms");

    let args: Vec<String> = env::args().collect();

    let config = Config::new(&args).unwrap_or_else(|err| {
        println!("Problem parsing arguments: {}", err);
        process::exit(1);
    });

    println!(
        "Host:{} BatchSize:{} Iterations:{} Timeout:{}",
        config.addr, config.b, config.n, config.timeout
    );
    println!("==============================================================================================================================================");

    let (tx, rx) = mpsc::channel::<Metrics>();
    let start = Instant::now();

    {
        let config = config.clone();
        let tx = tx.clone();

        tokio::spawn(async move {
            let metrics = execute(&config).await.expect("ERROR!");
            report(&config, &metrics);
            tx.send(metrics).unwrap();
        });
    }

    let result = rx.recv().unwrap();

    let mut metrics = Metrics::default();
    metrics.lat.extend(result.lat.iter());
    metrics.model_lat.extend(result.model_lat.iter());
    metrics.total_secs = start.elapsed().as_secs_f32();

    tokio::time::sleep(Duration::from_secs(1)).await;

    report(&config, &metrics);

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
    let lats = latencies.iter().skip(skip).take(take);

    let mean = lats.clone().sum::<u64>() / (i - skip) as u64;
    let max = *lats.clone().max().unwrap();
    let count = lats.clone().collect::<Vec<&u64>>().len();
    let timeouts = lats
        .clone()
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

fn report(config: &Config, metrics: &Metrics) {
    println!("REPORT ==================================================================================================================================");
    log_stats(
        "> Model",
        config.n,
        config.n,
        metrics.total_secs,
        config.timeout,
        &metrics.model_lat,
        0,
        config.n,
    );
    println!("=========================================================================================================================================");
}
