use std::env::args;
use std::error::Error;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tokio;

pub mod cb {
    tonic::include_proto!("cb");
}

const REPORT: usize = 100000;

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[derive(Debug, Clone)]
struct Params {
    host: String,
    b: usize,
    n: usize,
    timeout: u64,
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

    async fn execute(&mut self, params: &Params) -> Result<Metrics, Box<dyn Error>> {
        let mut client =
            cb::inference_client::InferenceClient::connect(params.host.clone()).await?;

        let request = cb::PredictRequest {
            features: vec![
                cb::Features {
                    float_feature1: 0.55,
                    float_feature2: 0.33,
                    cat_feature1: "A".to_string(),
                    cat_feature2: "B".to_string(),
                    cat_feature3: "C".to_string(),
                };
                params.b
            ],
        };

        // warm up 10 times
        for _ in 0..10 {
            _ = client.predict(request.clone()).await?;
        }

        let mut report_start = Instant::now();
        let mut total_secs = 0.0f32;
        let mut lat = vec![0u64; params.n];
        let mut model_lat = vec![0u64; params.n];

        for i in 1..params.n {
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
                    params.n,
                    secs,
                    params.timeout,
                    &model_lat,
                    i - REPORT,
                    REPORT,
                );
                log_stats(
                    &self.id,
                    i,
                    params.n,
                    secs,
                    params.timeout,
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

    let host = args().nth(1).expect("http://host:port missing");
    let users: usize = args()
        .nth(2)
        .expect("missing number users")
        .parse()
        .unwrap();
    let b: usize = args()
        .nth(3)
        .expect("missing number batch_size")
        .parse()
        .unwrap();
    let n: usize = args()
        .nth(4)
        .expect("missing number iterations")
        .parse()
        .unwrap();
    let timeout: u64 = args().nth(5).expect("missing timeout_ms").parse().unwrap();

    println!(
        "Host:{} Users:{} BatchSize:{} Iterations:{} Timeout:{}",
        host.clone(),
        users,
        b,
        n,
        timeout
    );
    println!("========================================================================================================================================================");

    let params = Params {
        host,
        b,
        n,
        timeout,
    };
    let (tx, rx) = mpsc::channel::<Metrics>();
    let start = Instant::now();

    for u in 0..users {
        let params = params.clone();
        let tx = tx.clone();
        let id = format!("User{:0>2}", u);

        tokio::spawn(async move {
            let metrics = User::new(id.clone())
                .execute(&params)
                .await
                .expect("ERROR!");
            report(&id, 1, &params, &metrics);
            tx.send(metrics).unwrap();
        });
    }

    let mut metrics = Metrics::default();
    for _ in 0..users {
        let result = rx.recv().unwrap();

        metrics.lat.extend(result.lat.iter());
        metrics.model_lat.extend(result.model_lat.iter());
        metrics.total_secs = start.elapsed().as_secs_f32();
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
    report("TEST  ", users, &params, &metrics);

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

fn report(id: &str, users: usize, params: &Params, metrics: &Metrics) {
    println!("REPORT =================================================================================================================================================");
    log_stats(
        ">Model",
        users * params.n,
        users * params.n,
        metrics.total_secs,
        params.timeout,
        &metrics.model_lat,
        0,
        users * params.n,
    );
    log_stats(
        &id,
        users * params.n,
        users * params.n,
        metrics.total_secs,
        params.timeout,
        &metrics.lat,
        0,
        users * params.n,
    );
    println!("========================================================================================================================================================");
}
