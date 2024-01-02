use reqwest::{
    blocking::{Client, RequestBuilder},
    header::USER_AGENT,
    Url,
};
use serde_json::{json, Value};
use std::collections::HashMap;

const API_VERSION: &str = "4.43.0";

fn build_request<'a>(
    client: &Client,
    verbe: &'a str,
    route: &str,
    mut qs_params: HashMap<&str, &'a str>,
    json_params: Value,
    token: &str,
) -> RequestBuilder {
    let BASE_URL = Url::parse("https://api.ecoledirecte.com/").unwrap();
    qs_params.insert("verbe", verbe);
    qs_params.insert("v", API_VERSION);
    let url = Url::parse_with_params(BASE_URL.join(route).unwrap().as_str(), qs_params).unwrap();
    client
        .post(url)
        .header(USER_AGENT, "ecoledirecte-imap")
        .header("X-Token", token)
        .body("data=".to_owned() + &json_params.to_string())
}

pub fn login(
    client: &Client,
    username: &str,
    password: &str,
) -> Result<(u32, String), Option<String>> {
    let request = build_request(
        client,
        "",
        "/v3/login.awp",
        HashMap::new(),
        json!({
            "identifiant": username,
            "motdepasse": password,
        }),
        "",
    );
    let response: Value = request.send().unwrap().json().unwrap();

    if response["code"] == json!(200) {
        Ok((
            response["data"]["accounts"][0]["id"]
                .as_u64()
                .unwrap()
                .try_into()
                .unwrap(),
            response["token"].as_str().unwrap().to_string(),
        ))
    } else {
        Err(response["message"].as_str().map(|s: &str| s.to_string()))
    }
}

pub fn get_folder_info(client: &Client, mailbox_id: u32, user_id: u32, token: &str) -> Value {
    let mailbox_id = mailbox_id.to_string();
    let request = build_request(
        client,
        "get",
        &format!("/v3/eleves/{user_id}/messages.awp"),
        {
            let mut qs = HashMap::<&str, &str>::new();
            qs.insert("idClasseur", &mailbox_id);
            qs
        },
        json!({}),
        token,
    );
    request.send().unwrap().json::<Value>().unwrap()["data"].take()
}

pub fn get_folders(client: &Client, id: u32, token: &str) -> Vec<(String, u32)> {
    get_folder_info(client, 0, id, token)["classeurs"]
        .as_array()
        .unwrap()
        .into_iter()
        .map(|classeur| {
            (
                classeur["libelle"].as_str().unwrap().to_string(),
                classeur["id"].as_u64().unwrap() as u32,
            )
        })
        .collect()
}

pub fn get_folder_messages(client: &Client, mailbox_id: u32, message_type: &str, page: (u32, u32), user_id: u32, token: &str) -> Value {
    let mailbox_id = mailbox_id.to_string();
    let request = build_request(
        client,
        "get",
        &format!("/v3/eleves/{user_id}/messages.awp"),
        {
            let mut qs = HashMap::<&str, &str>::new();
            qs.insert("idClasseur", &mailbox_id);
            qs.insert("typeRecuperation", message_type);
            qs.insert("page", &page.0);
            qs.insert("itemsPerPage", &page.1);
            qs
        },
        json!({}),
        token,
    );
    request.send().unwrap().json::<Value>().unwrap()["data"].take()
}
