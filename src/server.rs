use tonic::{transport::Server, Request, Response, Status};

pub mod cb {
    tonic::include_proto!("cb");
}

mod predict;
use predict::predict;

#[derive(Debug, Default)]
pub struct CatboostInferenceService {}

#[tonic::async_trait]
impl cb::inference_server::Inference for CatboostInferenceService {
    async fn predict(
        &self,
        request: Request<cb::PredictRequest>,
    ) -> Result<Response<cb::PredictResponse>, Status> {
        let reply = predict(request.into_inner());

        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:50051".parse()?;
    let service = CatboostInferenceService::default();

    let server = Server::builder()
        .add_service(cb::inference_server::InferenceServer::new(service))
        .serve(addr)
        .await?;

    println!("SERVER: {:?}", server);

    Ok(())
}
