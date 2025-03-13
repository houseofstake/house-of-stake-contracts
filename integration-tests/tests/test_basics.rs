mod setup;

use crate::setup::VenearTestWorkspaceBuilder;

#[tokio::test]
async fn test_deploy_venear_and_account_with_lockup() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let user = v.create_account_with_lockup().await?;

    let account_info = v.account_info(user.id()).await?;

    println!("Account info: {:#?}", account_info);

    Ok(())
}
