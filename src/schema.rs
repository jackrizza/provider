// @generated automatically by Diesel CLI.

diesel::table! {
    auth (id) {
        id -> Nullable<Text>,
        email -> Text,
        password -> Text,
        refresh_token -> Text,
        access_token -> Text,
        refresh_token_expires_at -> Text,
        access_token_expires_at -> Text,
        state -> Text,
        last_error -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    entities (id) {
        id -> Nullable<Text>,
        source -> Text,
        tags -> Text,
        data -> Text,
        etag -> Text,
        fetched_at -> Text,
        refresh_after -> Text,
        state -> Text,
        last_error -> Text,
        updated_at -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(auth, entities,);
