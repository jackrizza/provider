// @generated automatically by Diesel CLI.

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
