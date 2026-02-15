use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── Add new columns to users table (one per ALTER for SQLite compat) ──
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(
                        ColumnDef::new(Users::EmailVerified)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(ColumnDef::new(Users::TotpSecret).string().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(
                        ColumnDef::new(Users::TotpEnabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(
                        ColumnDef::new(Users::FailedLoginAttempts)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(ColumnDef::new(Users::LockedUntil).timestamp().null())
                    .to_owned(),
            )
            .await?;

        // ── Create refresh_tokens table ──
        manager
            .create_table(
                Table::create()
                    .table(RefreshTokens::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RefreshTokens::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(RefreshTokens::UserId).integer().not_null())
                    .col(
                        ColumnDef::new(RefreshTokens::TokenHash)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(RefreshTokens::ExpiresAt)
                            .timestamp()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RefreshTokens::Revoked)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(RefreshTokens::IpAddress).string().null())
                    .col(ColumnDef::new(RefreshTokens::UserAgent).string().null())
                    .col(
                        ColumnDef::new(RefreshTokens::CreatedAt)
                            .timestamp()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // ── Create sessions table ──
        manager
            .create_table(
                Table::create()
                    .table(Sessions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Sessions::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Sessions::UserId).integer().not_null())
                    .col(ColumnDef::new(Sessions::TokenHash).string().not_null())
                    .col(ColumnDef::new(Sessions::ExpiresAt).timestamp().not_null())
                    .col(
                        ColumnDef::new(Sessions::Revoked)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Sessions::IpAddress).string().null())
                    .col(ColumnDef::new(Sessions::UserAgent).string().null())
                    .col(ColumnDef::new(Sessions::CreatedAt).timestamp().not_null())
                    .to_owned(),
            )
            .await?;

        // ── Create security_tokens table ──
        manager
            .create_table(
                Table::create()
                    .table(SecurityTokens::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SecurityTokens::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(SecurityTokens::UserId).integer().not_null())
                    .col(
                        ColumnDef::new(SecurityTokens::TokenHash)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(SecurityTokens::TokenType)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SecurityTokens::ExpiresAt)
                            .timestamp()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SecurityTokens::Used)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(SecurityTokens::CreatedAt)
                            .timestamp()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // ── Create login_events table ──
        manager
            .create_table(
                Table::create()
                    .table(LoginEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LoginEvents::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(LoginEvents::UserId).integer().null())
                    .col(ColumnDef::new(LoginEvents::Email).string().not_null())
                    .col(ColumnDef::new(LoginEvents::Success).boolean().not_null())
                    .col(ColumnDef::new(LoginEvents::FailureReason).string().null())
                    .col(ColumnDef::new(LoginEvents::IpAddress).string().null())
                    .col(ColumnDef::new(LoginEvents::UserAgent).string().null())
                    .col(
                        ColumnDef::new(LoginEvents::CreatedAt)
                            .timestamp()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(LoginEvents::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(SecurityTokens::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Sessions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(RefreshTokens::Table).to_owned())
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::EmailVerified)
                    .drop_column(Users::TotpSecret)
                    .drop_column(Users::TotpEnabled)
                    .drop_column(Users::FailedLoginAttempts)
                    .drop_column(Users::LockedUntil)
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
enum Users {
    Table,
    EmailVerified,
    TotpSecret,
    TotpEnabled,
    FailedLoginAttempts,
    LockedUntil,
}

#[derive(Iden)]
enum RefreshTokens {
    Table,
    Id,
    UserId,
    TokenHash,
    ExpiresAt,
    Revoked,
    IpAddress,
    UserAgent,
    CreatedAt,
}

#[derive(Iden)]
enum Sessions {
    Table,
    Id,
    UserId,
    TokenHash,
    ExpiresAt,
    Revoked,
    IpAddress,
    UserAgent,
    CreatedAt,
}

#[derive(Iden)]
enum SecurityTokens {
    Table,
    Id,
    UserId,
    TokenHash,
    TokenType,
    ExpiresAt,
    Used,
    CreatedAt,
}

#[derive(Iden)]
enum LoginEvents {
    Table,
    Id,
    UserId,
    Email,
    Success,
    FailureReason,
    IpAddress,
    UserAgent,
    CreatedAt,
}
