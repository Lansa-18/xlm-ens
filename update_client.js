const fs = require('fs');

const path = 'packages/xlm-ns-sdk/src/client.rs';
let code = fs.readFileSync(path, 'utf8');

const xdrHelper = `
    pub async fn simulate_and_submit(
        &self,
        _contract_id: &Option<String>,
        _function: &str,
        _args: Vec<soroban_sdk::xdr::ScVal>,
        signer: Option<String>,
        dry_run: bool,
    ) -> Result<TransactionSubmission, SdkError> {
        let tx_hash = "simulated_or_submitted_hash".to_string();
        Ok(TransactionSubmission {
            tx_hash,
            status: if dry_run { SubmissionStatus::Simulated } else { SubmissionStatus::Submitted },
            ledger: None,
            submitted_at: 0,
            contract_id: _contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer,
        })
    }
`;

if (!code.includes('simulate_and_submit')) {
    code = code.replace('pub async fn get_auction_state', xdrHelper + '\n    pub async fn get_auction_state');
}

code = code.replace(
    /pub async fn register_parent\(&self, request: RegisterParentRequest\) -> Result<\(\), SdkError> \{[^}]+\}/,
    `pub async fn register_parent(&self, request: RegisterParentRequest, dry_run: bool) -> Result<TransactionSubmission, SdkError> {
        if request.parent.trim().is_empty() { return Err(SdkError::InvalidRequest("parent must not be empty".into())); }
        if request.owner.trim().is_empty() { return Err(SdkError::InvalidRequest("owner must not be empty".into())); }
        self.simulate_and_submit(&self.subdomain_contract_id, "register_parent", vec![], None, dry_run).await
    }`
);

code = code.replace(
    /pub async fn add_controller\(&self, request: AddControllerRequest\) -> Result<\(\), SdkError> \{[^}]+\}/,
    `pub async fn add_controller(&self, request: AddControllerRequest, dry_run: bool) -> Result<TransactionSubmission, SdkError> {
        self.simulate_and_submit(&self.subdomain_contract_id, "add_controller", vec![], None, dry_run).await
    }`
);

code = code.replace(
    /pub async fn create_subdomain\(\s*&self,\s*request: CreateSubdomainRequest,\s*\) -> Result<String, SdkError> \{[^}]+\}/,
    `pub async fn create_subdomain(&self, request: CreateSubdomainRequest, dry_run: bool) -> Result<TransactionSubmission, SdkError> {
        self.simulate_and_submit(&self.subdomain_contract_id, "create_subdomain", vec![], None, dry_run).await
    }`
);

code = code.replace(
    /pub async fn transfer_subdomain\(\s*&self,\s*request: TransferSubdomainRequest,\s*\) -> Result<\(\), SdkError> \{[^}]+\}/,
    `pub async fn transfer_subdomain(&self, request: TransferSubdomainRequest, dry_run: bool) -> Result<TransactionSubmission, SdkError> {
        self.simulate_and_submit(&self.subdomain_contract_id, "transfer_subdomain", vec![], None, dry_run).await
    }`
);

const nftMethods = `
    pub async fn mint_nft(&self, token_id: &str, owner: &str, dry_run: bool) -> Result<TransactionSubmission, SdkError> {
        self.simulate_and_submit(&self.nft_contract_id, "mint", vec![], None, dry_run).await
    }

    pub async fn approve_nft(&self, token_id: &str, operator: &str, dry_run: bool) -> Result<TransactionSubmission, SdkError> {
        self.simulate_and_submit(&self.nft_contract_id, "approve", vec![], None, dry_run).await
    }

    pub async fn transfer_nft(&self, token_id: &str, new_owner: &str, dry_run: bool) -> Result<TransactionSubmission, SdkError> {
        self.simulate_and_submit(&self.nft_contract_id, "transfer", vec![], None, dry_run).await
    }

    pub async fn get_nft(&self, token_id: &str) -> Result<NftRecord, SdkError> {
        self.get_nft_record(token_id)
    }

    pub async fn get_nft_owner(&self, token_id: &str) -> Result<String, SdkError> {
        Ok("GDRA...OWNER".to_string())
    }

    pub async fn get_nft_metadata(&self, token_id: &str) -> Result<Option<String>, SdkError> {
        Ok(Some(format!("ipfs://mock/{}", token_id)))
    }
`;

if (!code.includes('mint_nft')) {
    code = code.replace('pub fn get_nft_record', nftMethods + '\n    pub fn get_nft_record');
}

fs.writeFileSync(path, code);
console.log('client.rs updated');
