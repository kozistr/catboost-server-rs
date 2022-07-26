use std::thread;
use std::time::Instant;

use crate::cb::{Features, PredictRequest, PredictResponse, Prediction};

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

thread_local! {
    pub static MODEL: catboost::Model = load_model();
}

fn load_model() -> catboost::Model {
    println!("THRED:{:?} Loading Model...", thread::current().id());
    catboost::Model::load("model.cbm").unwrap()
}

pub fn preprocess(features: &Vec<Features>) -> (Vec<Vec<f32>>, Vec<Vec<String>>) {
    let float_features = features
        .iter()
        .map(|f| vec![f.float_feature1, f.float_feature2])
        .collect();

    let cat_features = features
        .iter()
        .map(|f| {
            vec![
                f.cat_feature1.clone(),
                f.cat_feature2.clone(),
                f.cat_feature3.clone(),
            ]
        })
        .collect();

    (float_features, cat_features)
}

pub fn predict(request: PredictRequest) -> PredictResponse {
    let (float_features, cat_features) = preprocess(&request.features);

    let start = Instant::now();
    let pred = MODEL.with(|model| {
        model
            .calc_model_prediction(float_features, cat_features)
            .unwrap()
    });
    let model_latency = start.elapsed().as_nanos() as u64;

    PredictResponse {
        predictions: pred
            .iter()
            .map(|score| Prediction {
                score: *score as f32,
            })
            .collect(),
        model_latency,
    }
}
