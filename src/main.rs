use std::{
    fs::File,
    sync::{Arc, Mutex},
};

use axum::{
    body::{Body, HttpBody},
    extract::State,
    http::{response, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use axum_macros;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tower_http::services::ServeFile;

#[tokio::main]
async fn main() {
    let mut state = AppState {
        tracked_users: Arc::new(Mutex::new(Vec::new())),
        posts: Arc::new(Mutex::new(Vec::new())),
    };

    state.posts.lock().expect("").push(Post {
        poster: "example1".into(),
        contents: "hello, world!".into(),
    });
    state.posts.lock().expect("").push(Post {
        poster: "example2".into(),
        contents: "greetings, earth!".into(),
    });
    state.posts.lock().expect("").push(Post {
        poster: "example3".into(),
        contents: "howdy, land!".into(),
    });
    state.posts.lock().expect("").push(Post {
        poster: "example4".into(),
        contents: "hey".into(),
    });

    let app = Router::new()
        .route("/", get(hello))
        .route("/posts", get(posts))
        .route("/post", post(user_post))
        .route("/register", post(register))
        .route("/login", post(login))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn hello(headers: HeaderMap) -> Response {
    let name = get_header_value(&headers, "User-Agent").unwrap_or("world".into());

    let content_type = get_header_value(&headers, "Accept").unwrap_or("text/plain".into());

    match &*content_type {
        "text/html" => format!("<p>Hello, {}!</p>", name).into_response(),
        "text/plain" => format!("Hello, {}!", name).into_response(),
        "application/json" => Json(Message {
            message: format!("Hello, {}!", name),
        })
        .into_response(),
        _ => response::Builder::new()
            .status(400)
            .body(Body::from(()))
            .unwrap(),
    }
}

async fn posts(headers: HeaderMap, state: State<AppState>) -> Response {
    let content_type = get_header_value(&headers, "Accept").unwrap_or("text/plain".into());

    let mut output = String::new();

    let data = state.posts.lock().expect("poisoned data lock");

    match &*content_type {
        "text/html" => {
            for p in &*data {
                output += format!(
                    "<div class=\"post\">{}: \"{}\"</div>\n",
                    p.poster, p.contents
                )
                .as_str();
            }
            output.into_response()
        }
        "text/plain" => {
            for p in &*data {
                output += format!("{}: \"{}\"\n", p.poster, p.contents).as_str();
            }
            output.into_response()
        }
        "application/json" => Json(&*data).into_response(),
        _ => response::Builder::new()
            .status(400)
            .body(().into())
            .unwrap(),
    }
}

#[axum_macros::debug_handler(state = AppState)]
async fn user_post(headers: HeaderMap, state: State<AppState>, body: String) -> Response {
    let Ok(req) = serde_json::from_str::<Message>(body.as_str()) else {
        return response::Builder::new()
            .status(400)
            .body(().into())
            .unwrap();
    };

    let Some(auth) = get_header_value(&headers, "Authorization").map(|s| String::from(s)) else {
        return response::Builder::new()
            .status(401)
            .body("Authorization required".into())
            .unwrap();
    };

    let user_data = state.tracked_users.lock().expect("poisoned data lock");
    let mut post_data = state.posts.lock().expect("poisoned data lock");

    let Some(user) = user_data.iter().find(|user| user.token.eq(&auth)) else {
        return response::Builder::new()
            .status(401)
            .body("Invalid Authorization".into())
            .unwrap();
    };

    post_data.push(Post {
        poster: user.username.clone(),
        contents: req.message,
    });

    response::Builder::new()
        .status(201)
        .body(().into())
        .unwrap()
}

async fn register(state: State<AppState>, body: String) -> Response {
    let Ok(req) = serde_json::from_str::<LoginRequest>(body.as_str()) else {
        return response::Builder::new()
            .status(400)
            .body("Request failed to parse".into())
            .unwrap();
    };

    let mut data = state.tracked_users.lock().expect("poisoned data lock");

    if data.iter().any(|user| user.username == req.username) {
        return response::Builder::new()
            .status(403)
            .body("username already exists".into())
            .unwrap();
    }

    let token: String = base64::prelude::BASE64_STANDARD
        .encode((0..10).map(|_| rand::random::<u8>()).collect::<Vec<u8>>());

    data.push(User {
        username: req.username.clone(),
        password: req.password,
        token: token.clone(),
    });

    response::Builder::new()
        .status(201)
        .body(
            serde_json::to_string(&LoginResponse {
                username: req.username,
                token,
            })
            .unwrap()
            .into(),
        )
        .unwrap()
}

async fn login(state: State<AppState>, body: String) -> Response {
    let Ok(req) = serde_json::from_str::<LoginRequest>(body.as_str()) else {
        return response::Builder::new()
            .status(400)
            .body("Request failed to parse".into())
            .unwrap();
    };

    let mut data = state.tracked_users.lock().expect("poisoned data lock");

    let Some(user) = data.iter().find(|user| user.username == req.username) else {
        return response::Builder::new()
            .status(401)
            .body("User not found".into())
            .unwrap();
    };

    if (user.password == req.password) {
        return response::Builder::new()
            .status(200)
            .body(
                serde_json::to_string(&LoginResponse {
                    username: req.username,
                    token: user.token.clone(),
                })
                .unwrap()
                .into(),
            )
            .unwrap();
    } else {
        return response::Builder::new()
            .status(401)
            .body("Invalid credentials".into())
            .unwrap();
    }
}

#[derive(Serialize, Deserialize)]
struct Message {
    message: String,
}

#[derive(Clone)]
struct AppState {
    tracked_users: Arc<Mutex<Vec<User>>>,
    posts: Arc<Mutex<Vec<Post>>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Post {
    poster: String,
    contents: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct User {
    username: String,
    password: String,
    token: String,
}

#[derive(Serialize, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
struct LoginResponse {
    username: String,
    token: String,
}

fn get_header_value(headers: &HeaderMap, target: &str) -> Option<Box<str>> {
    headers
        .get(target)
        .map(|val| Box::from(val.to_str().unwrap_or("")))
}
