use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenHolder {
    pub address: Pubkey,
    pub owner: Pubkey,
    pub balance: f64,
    pub percentage: f64,
    pub sol_balance: f64,
    pub wsol_balance: f64,
    pub transfer_history: Vec<TransferInfo>,
    pub unique_senders: HashSet<Pubkey>,
}

#[derive(Debug, Serialize, Deserialize,Clone)]
pub struct TokenInfo {
    pub mint: Pubkey,
    pub total_supply: f64,
    pub holders: Vec<TokenHolder>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransferInfo {
    pub from: Pubkey,
    pub to: Pubkey,
    pub amount: f64,
    pub timestamp: i64,
}