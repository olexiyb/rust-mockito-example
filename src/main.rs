use hyper::service::{make_service_fn, service_fn};
use hyper::{
    body::to_bytes, client::HttpConnector, Body, Client, Method, Request, Response, Server,
    StatusCode,
};
use hyper_tls::HttpsConnector;
use serde_derive::{Deserialize, Serialize};
use serde_json::from_slice;

#[cfg(test)]
use mockito;

#[cfg(not(test))]
const CATS_URL: &str = "https://cat-fact.herokuapp.com";

#[cfg(not(test))]
const TODO_URL: &str = "https://jsonplaceholder.typicode.com";

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
type Result<T> = std::result::Result<T, Error>;
type HttpClient = Client<HttpsConnector<HttpConnector>>;

#[derive(Serialize, Deserialize)]
struct CatFact {
    text: String,
}

#[derive(Serialize, Deserialize)]
struct TODO {
    title: String,
}

fn get_cats_url() -> String {
    #[cfg(not(test))]
    let url = format!("{}/facts/random", CATS_URL);
    #[cfg(test)]
    let url = format!("{}/facts/random", mockito::server_url());
    url
}

fn get_todo_url() -> String {
    #[cfg(not(test))]
    let url = format!("{}/todos/1", TODO_URL);
    #[cfg(test)]
    let url = format!("{}/todos/1", mockito::server_url());
    url
}

async fn basic(_req: Request<Body>, client: &HttpClient) -> Result<Body> {
    let res = do_get_req(&get_todo_url(), &client).await?;
    let body = to_bytes(res.into_body()).await?;
    let todo: TODO = from_slice(&body)?;
    Ok(todo.title.into())
}

async fn double(_req: Request<Body>, client: &HttpClient) -> Result<Body> {
    let res_todo = do_get_req(&get_todo_url(), &client).await?;
    let body_todo = to_bytes(res_todo.into_body()).await?;
    let todo: TODO = from_slice(&body_todo)?;

    let res_cats = do_get_req(&get_cats_url(), &client).await?;
    let body_cats = to_bytes(res_cats.into_body()).await?;
    let fact: CatFact = from_slice(&body_cats)?;
    Ok(format!("Todo: {}, Cat Fact: {}", todo.title, fact.text).into())
}

async fn do_get_req(uri: &str, client: &HttpClient) -> Result<Response<Body>> {
    let request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())?;
    let res = client.request(request).await?;
    Ok(res)
}

async fn route(req: Request<Body>, client: HttpClient) -> Result<Response<Body>> {
    let mut response = Response::new(Body::empty());

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/basic") => {
            *response.body_mut() = basic(req, &client).await?;
        }
        (&Method::GET, "/double") => {
            *response.body_mut() = double(req, &client).await?;
        }
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    };
    Ok(response)
}

fn init_client() -> HttpClient {
    let https = HttpsConnector::new();
    Client::builder().build::<_, Body>(https)
}

async fn run_server() -> Result<()> {
    let client = init_client();

    let new_service = make_service_fn(move |_| {
        let client_clone = client.clone();
        async { Ok::<_, Error>(service_fn(move |req| route(req, client_clone.clone()))) }
    });
    let addr = "127.0.0.1:3000".parse().unwrap();
    let server = Server::bind(&addr).serve(new_service);

    println!("Listening on http://{}", addr);
    let res = server.await?;
    Ok(res)
}

#[tokio::main]
async fn main() -> Result<()> {
    run_server().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::mock;
    use tokio::runtime::Runtime;

    #[test]
    fn test_basic() {
        let _mt = mock("GET", "/todos/1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"title": "get another cat"}"#)
            .create();

        let mut rt = Runtime::new().unwrap();
        let client = init_client();

        // start server
        rt.spawn(run_server());

        // wait for server to come up
        std::thread::sleep(std::time::Duration::from_millis(50));

        // make requests
        let req_fut = client.request(
            Request::builder()
                .method(Method::GET)
                .uri("http://localhost:3000/basic")
                .body(Body::empty())
                .unwrap(),
        );
        let res = rt.block_on(req_fut).unwrap();
        let body = rt.block_on(to_bytes(res.into_body())).unwrap();

        assert_eq!(std::str::from_utf8(&body).unwrap(), "get another cat");
    }

    #[test]
    fn test_double() {
        let mut rt = Runtime::new().unwrap();
        let client = init_client();
        let _mc = mock("GET", "/facts/random")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"text": "cats are the best living creatures in the universe"}"#)
            .create();

        let _mt = mock("GET", "/todos/1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"title": "get another cat"}"#)
            .create();

        // start server
        rt.spawn(run_server());

        // wait for server to come up
        std::thread::sleep(std::time::Duration::from_millis(50));

        // make requests
        let req_fut = client.request(
            Request::builder()
                .method(Method::GET)
                .uri("http://localhost:3000/double")
                .body(Body::empty())
                .unwrap(),
        );
        let res = rt.block_on(req_fut).unwrap();
        let body = rt.block_on(to_bytes(res.into_body())).unwrap();

        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            "Todo: get another cat, Cat Fact: cats are the best living creatures in the universe"
        );
    }
}
