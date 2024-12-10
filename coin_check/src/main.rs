use abi::{TokenHolder, TokenInfo};
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use spl_token_2022::state::Account;
use spl_token_2022::extension::StateWithExtensions;
// use solana_transaction_status::UiTransactionEncoding;
// use solana_sdk::signature::Signature;
// use abi::TransferInfo;
// use solana_transaction_status::EncodedTransaction;
// use solana_transaction_status::UiMessage;
use solana_sdk::commitment_config::CommitmentConfig;
// use solana_client::rpc_config::RpcTransactionConfig;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use std::collections::HashMap;
use reqwest::Client;
use serde_json;
use regex;
use warp::Filter;
use std::env;
use log;
use log::debug;
use std::sync::Mutex;
use std::time::Duration;

lazy_static::lazy_static! {
    static ref CACHE: Mutex<HashMap<String, (bool, Instant)>> = Mutex::new(HashMap::new());
}

pub async fn get_top_holders(token_address: &str) -> Result<TokenInfo> {
    let api_key = env::var("API_KEY").expect("API_KEY must be set");
    let rpc_url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);
    let client = Arc::new(RpcClient::new_with_commitment(
        rpc_url,
        CommitmentConfig::confirmed(),
    ));

    let token_pubkey = Pubkey::from_str(token_address)?;
    let largest_accounts = client.get_token_largest_accounts(&token_pubkey)?;

    let mut holders = Vec::new();
    let total_supply = 0.0;

    let mut tasks = Vec::new();

    let whitelist_addresses: Vec<String> = env::var("WHITELIST_ADDRESSES")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    for account in largest_accounts {
        let account_pubkey = account.address.parse::<Pubkey>()?;
        
        // 检查是否在白名单中
        if whitelist_addresses.contains(&account_pubkey.to_string()) {
            continue; // 跳过白名单中的地址
        }

        let client_clone = Arc::clone(&client);
        let api_key_clone = api_key.clone(); // 克隆 api_key 以避免移动

        let task: tokio::task::JoinHandle<Result<(Pubkey, Pubkey, f64, Pubkey, HashSet<Pubkey>), anyhow::Error>> = tokio::spawn(async move {
            let balance = client_clone.get_token_account_balance(&account_pubkey)?;
            let account_info = client_clone.get_account(&account_pubkey)?;
            let token_account = StateWithExtensions::<Account>::unpack(&account_info.data)?;
            let amount = balance.ui_amount.unwrap_or(0.0);

            // 仅当 amount 于于 300000000 时才检查转账历史
            if amount <= 300000000.0 {
                let transfer_history = get_sol_transfer_history_api(&token_account.base.owner.to_string(), &api_key_clone).await?; // 使用克隆的 api_key
                let unique_senders = if transfer_history != Pubkey::default() {
                    HashSet::from([transfer_history])
                } else {
                    HashSet::new()
                };

                Ok((account_pubkey, token_account.base.owner, amount, transfer_history, unique_senders))
            } else {
                Ok((account_pubkey, token_account.base.owner, amount, Pubkey::default(), HashSet::new()))
            }
        });

        tasks.push(task);
    }

    let results = futures::future::try_join_all(tasks).await?;

    for result in results {
        match result {
            Ok((account_pubkey, owner, amount, _, unique_senders)) => {
                holders.push(TokenHolder {
                    address: account_pubkey,
                    owner,
                    balance: amount,
                    percentage: 0.0,
                    sol_balance: 0.0,
                    wsol_balance: 0.0,
                    transfer_history: Vec::new(),
                    unique_senders: unique_senders.clone(),
                });
            },
            Err(e) => {
                debug!("Error processing account: {:?}", e);
            }
        }
    }

    // 计算百分比
    for holder in &mut holders {
        holder.percentage = (holder.balance / total_supply) * 100.0;
    }

    Ok(TokenInfo {
        mint: token_pubkey,
        total_supply,
        holders,
    })
}

// pub async fn get_sol_transfer_history(address: &Pubkey, client: &RpcClient) -> Result<Vec<TransferInfo>> {
//     let signatures = client.get_signatures_for_address(address)?;
//     let mut transfers = Vec::new();
//     let mut unique_senders = HashSet::new();

//     for sig_info in signatures.iter().take(100) { // 限制查询最近50条记录
//         let config = RpcTransactionConfig {
//             encoding: Some(UiTransactionEncoding::Json),
//             commitment: Some(CommitmentConfig::confirmed()),
//             max_supported_transaction_version: Some(0),
//         };

//         let tx = client.get_transaction_with_config(
//             &sig_info.signature.parse::<Signature>()?,
//             config,
//         );

//         match tx {
//             Ok(transaction) => {
//                 if let Some(meta) = transaction.transaction.meta {
//                     for (i, post_balance) in meta.post_balances.iter().enumerate() {
//                         let pre_balance = meta.pre_balances[i];
//                         if pre_balance < *post_balance {
//                             // SOL 转入
//                             let account_keys = match &transaction.transaction.transaction {
//                                 EncodedTransaction::Json(ui_tx) => match &ui_tx.message {
//                                     UiMessage::Raw(message) => &message.account_keys,
//                                     UiMessage::Parsed(message) => {
//                                         &message.account_keys.iter().map(|key| key.pubkey.clone()).collect::<Vec<String>>()
//                                     },
//                                 },
//                                 EncodedTransaction::Accounts(accounts) => {
//                                     &accounts.account_keys.iter().map(|key| key.pubkey.clone()).collect::<Vec<String>>()
//                                 },
//                                 _ => continue, // 跳过其他编码类型
//                             };

//                             let from_address = Pubkey::from_str(&account_keys[0]).unwrap();
//                             if from_address != *address { // 排除与��地址相同的转账地址
//                                 transfers.push(TransferInfo {
//                                     from: from_address,
//                                     to: *address,
//                                     amount: (*post_balance - pre_balance) as f64 / 1e9,
//                                     timestamp: sig_info.block_time.unwrap_or(0),
//                                 });

//                                 // 将转入址添加到 HashSet
//                                 unique_senders.insert(from_address);
//                                 return Ok(transfers); // 结束整个函数并返回结果
//                             }
//                         }
//                     }
//                 }
//             },
//             Err(e) => {
//                 eprintln!("Error fetching transaction: {:?}", e); // 打印错误信息
//             }
//         }
//     }

//     Ok(transfers)
// }

pub async fn get_sol_transfer_history_api(address: &str, api_key: &str) -> Result<Pubkey> {
    let api_url = format!("https://api.helius.xyz/v0/addresses/{}/transactions?api-key={}", address, api_key);
    let client = Client::new();

    let start_time = Instant::now(); // 开始计时
    let response = client.get(&api_url).send().await?;
    let response_data: Vec<serde_json::Value> = response.json().await?; // 解析 JSON
    debug!("response_data 的长度: {}", response_data.len()); // 打印 response_data 的长度
    let json_start_time = Instant::now(); // 开始计时解析 JSON
    let json_duration = json_start_time.elapsed(); // 结束计时
    debug!("解析 JSON 耗时: {:?}", json_duration); // 打印解析 JSON 的耗时

    let max_transactions: usize = env::var("MAX_TRANSACTIONS")
        .unwrap_or_else(|_| "10".to_string()) // 默认值为 10
        .parse()
        .expect("MAX_TRANSACTIONS must be a valid number");

    for tx in response_data.iter().take(max_transactions) { // 处理前 MAX_TRANSACTIONS 个交易
        if let Some(description) = tx.get("description") {
            if let Some(description_str) = description.as_str() {
                // 使用正则表达式提取来源地址和目标地址
                let re = regex::Regex::new(r"(\w{32,44}) transferred (\d+\.?\d*) SOL to (\w{32,44})").unwrap(); // 匹配来源地址和目标地址
                if let Some(captures) = re.captures(description_str) {
                    let from_address_str = &captures[1]; // 获取捕获的来源地址
                    let to_address_str = &captures[3]; // 获取捕获的目标地址

                    if to_address_str == address { // 检查标���址是否是本地址
                        return Ok(Pubkey::from_str(from_address_str)?); // 返回来源地址
                    }
                }
            }
        }
    }

    let total_duration = start_time.elapsed(); // 结束计时
    debug!("处理所有交易总耗时: {:?}", total_duration); // 使用 debug 日志打印总耗时

    Err(anyhow::anyhow!("未找到符合条件的地址")) // 如果没有找到，返回错误
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok(); // 加载 .env 文件
    let listen_address = env::var("LISTEN_ADDRESS").unwrap_or_else(|_| "127.0.0.1".to_string());
    let listen_port = env::var("LISTEN_PORT").unwrap_or_else(|_| "3030".to_string()).parse::<u16>()?;

    let health_check = warp::path("health_check")
        .and(warp::query::<HashMap<String, String>>()) // 接收查询参数
        .and_then(handle_health_check);

    let routes = health_check.with(warp::cors().allow_any_origin());

    println!("服务正在运行，监听在 {} 端口...", listen_port);
    warp::serve(routes).run((listen_address.parse::<std::net::IpAddr>()?, listen_port)).await;

    Ok(())
}

async fn handle_health_check(params: HashMap<String, String>) -> Result<impl warp::Reply, warp::Rejection> {
    let token_address = match params.get("token_address") {
        Some(address) => address.to_string(),
        None => return Ok(warp::reply::json(&None::<()>,)), // 返回 None
    };

    {
        let cache = CACHE.lock().unwrap();
        if let Some((is_healthy, timestamp)) = cache.get(&token_address) {
            if timestamp.elapsed() < Duration::from_secs(300) { // 检查缓存是否在5分钟内
                return Ok(warp::reply::json(is_healthy)); // 返回缓存的健康状态
            }
        }
    }

    let token_info = get_top_holders(&token_address).await.map_err(|_| warp::reject())?;

    let mut total_balance_map = HashMap::new();
    let skip_addresses: Vec<String> = env::var("SKIP_ADDRESSES")
        .unwrap_or_else(|_| {
            "5tzFkiKscXHK5ZXCGbXZxdw7gTjjD1mBwuoFbhUvuAi9,ASTyfSima4LLAdDgoFGkgqoKowG1LZFDr9fAQrg7iaJZ,AC5RDfQFmDS1deWZos921JfqscXdByf8BKHs5ACWjtW2,5VCwKtCXgCJ6kit5FybXjvriW3xELsFDhYrPSqtJNmcD,FWznbcNXWQuHTawe9RxvQ2LdCENssh12dsznf4RiouN5,H8sMJSCQxfKiFTCfDR3DUMLPwcRbM61LGFJ8N4dK3WjS,u6PJ8DtQuPFnfmwHbGFULQ4u4EgjDiyYKjVEsynXq2w,2AQdpHJ2JpcEgPiATUXjQxA8QmafFegfQwSLWSprPicm".to_string()
        })
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let mut is_healthy = true;

    for holder in &token_info.holders {
        if let Some(sender) = holder.unique_senders.iter().next() {
            let sender_str = sender.to_string();
            if skip_addresses.contains(&sender_str) {
                continue;
            }

            let balance = holder.balance;
            *total_balance_map.entry(sender_str).or_insert(0.0) += balance;
        }
    }

    let max_balance: f64 = env::var("MAX_BALANCE")
        .unwrap_or_else(|_| "300000000.0".to_string()) // 默认值为 300_000_000.0
        .parse()
        .expect("MAX_BALANCE must be a valid number");

    for (_, total_balance) in total_balance_map {
        if total_balance > max_balance { // 使用从环境变量读取的最大余额
            is_healthy = false;
            break; // 一旦现不健康，立即退出循环
        }
    }

    // 更新缓存
    let mut cache = CACHE.lock().unwrap();
    cache.insert(token_address.clone(), (is_healthy, Instant::now())); // 缓存健康状态和时间戳

    // 打印 token_address 和健康状态
    println!("Token Address: {}, Health Status: {:?}", token_address, is_healthy);

    Ok(warp::reply::json(&is_healthy)) // 返���健康状态
}

