use std::collections::{HashMap, HashSet};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde::de::Unexpected::Str;
use serde_json::Value;
use solana_program::pubkey::Pubkey;

pub(crate) struct NFTAsset {
    pub nft_address: String,
    pub owner_address: String,
}

pub(crate) struct TokenHolder {
    pub amount: u64,
    pub owner_address: String,
}

#[derive(Deserialize, Debug)]
struct Ownership {
    owner: String,
}

#[derive(Deserialize, Debug)]
struct Item {
    id: String,
    ownership: Ownership,
}

#[derive(Deserialize, Debug)]
struct GetAssetsResult {
    items: Vec<Item>,
    total: usize,
}

#[derive(Deserialize, Debug)]
struct GetAssetsResponse {
    result: GetAssetsResult,
}

#[derive(Deserialize, Debug)]
struct TokenAccount {
    owner: String,
    amount: u64,
}

#[derive(Deserialize, Debug)]
struct GetTokenAccountsResult {
    token_accounts: Vec<TokenAccount>,
    cursor: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GetTokenAccountsResponse {
    result: Option<GetTokenAccountsResult>,
}

pub(crate) async fn get_assets_by_group(
    client: &Client,
    helius_url: &str,
    collection: &Pubkey,
) -> Result<Vec<NFTAsset>, Box<dyn std::error::Error>> {
    let mut page = 1;
    let mut asset_list: Vec<NFTAsset> = Vec::new();
    let group_value = collection.to_string();

    println!("start fetching nft collection owners");

    loop {
        println!("fetching page: {}", page);
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "my-id",
            "method": "getAssetsByGroup",
            "params": {
                "groupKey": "collection",
                "groupValue": group_value,
                "page": page,
                "limit": 1000,
            }
        });

        let response = client
            .post(helius_url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?
            .json::<GetAssetsResponse>()
            .await?;

        let owners: Vec<NFTAsset> = response
            .result
            .items
            .into_iter()
            .map(|item| NFTAsset {
                nft_address: item.id,
                owner_address: item.ownership.owner,
            })
            .collect();

        asset_list.extend(owners);

        println!("total results in page: {}", response.result.total);

        if response.result.total != 1000 {
            break;
        } else {
            page += 1;
        }
    }

    Ok(asset_list)
}

pub(crate) async fn get_token_holders(
    client: &Client,
    helius_url: &str,
    mint: &Pubkey,
) -> Result<HashMap<String, u64>, Box<dyn std::error::Error>> {
    let mut page = 1;
    let mut all_owners: HashMap<String, u64> = HashMap::new();
    let mint_address = mint.to_string();
    let mut cursor : String = String::new();

    println!("start fetching token holders");

    loop {
        println!("fetching page: {}", page);

        let request_body;
        if cursor.is_empty() {
            request_body = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "getTokenAccounts",
                "id": "helius-test",
                "params": {
                    "page": page,
                    "limit": 1000,
                    "displayOptions": {},
                    "mint": mint_address,
                }
            });
        } else {
            request_body = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "getTokenAccounts",
                "id": "helius-test",
                "params": {
                    "page": page,
                    "limit": 1000,
                    "displayOptions": {},
                    "mint": mint_address,
                    "cursor": cursor,
                }
            });
        }

        let response = client
            .post(helius_url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?
            .json::<GetTokenAccountsResponse>()
            .await?;

        if let Some(result_data) = response.result {
            if result_data.token_accounts.is_empty() {
                println!("no more results, page: {}", page - 1);
                break;
            }

            println!("processing results, page {}", page);
            for account in result_data.token_accounts.iter() {
                let mut current_amount : u64;
                if all_owners.contains_key(&account.owner) {
                    current_amount = *all_owners.get(&account.owner).unwrap();
                } else {
                    current_amount = 0
                }
                current_amount += account.amount;
                all_owners.insert(account.owner.clone(), current_amount);
            }
            page += 1;
            if result_data.cursor.is_some() {
                cursor = result_data.cursor.unwrap();
            }
        } else {
            println!("No more results. Total pages: {}", page - 1);
            break;
        }
    }

    Ok(all_owners)
}
