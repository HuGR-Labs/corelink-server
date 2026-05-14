#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]
use std::net::SocketAddr;
use tonic::{transport::Server, Request, Response, Status};
use tracing::info;

pub mod health {
    tonic::include_proto!("corelink.health.v1");
}

use health::health_server::{Health, HealthServer};
use health::{health_check_response::ServingStatus, HealthCheckRequest, HealthCheckResponse};

struct HealthService;

#[tonic::async_trait]
impl Health for HealthService {
    async fn check(
        &self,
        _req: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: ServingStatus::Serving as i32,
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,corelink_server=debug".into()),
        )
        .init();

    let port = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50051u16);

    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;

    info!(%addr, "CoreLink server starting");

    Server::builder()
        .add_service(HealthServer::new(HealthService))
        // TODO semana 1: add_service(CasServer::new(CasService::new(...)))
        // TODO semana 1: add_service(ActionCacheServer::new(...))
        // TODO semana 1: add_service(ByteStreamServer::new(...))
        // TODO semana 1: add_service(CapabilitiesServer::new(...))
        .serve(addr)
        .await?;

    Ok(())
}
