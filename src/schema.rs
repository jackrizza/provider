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
        role -> Text,
    }
}

diesel::table! {
    blob (id) {
        id -> Nullable<Integer>,
        sha256_hex -> Text,
        size_bytes -> Integer,
        mime -> Nullable<Text>,
        content -> Binary,
    }
}

diesel::table! {
    blobs (id) {
        id -> Nullable<Integer>,
        sha256_hex -> Text,
        size_bytes -> Integer,
        mime -> Nullable<Text>,
        content -> Binary,
    }
}
diesel::table! {
    // use the actual primary key your FTS uses; many folks use `rowid`
    code_fts (rowid) {
        rowid -> Nullable<Integer>,

        // 👇 rename the Rust identifier, but point it at the real SQL column
        #[sql_name = "code_fts"]
        code_fts_ -> Nullable<Binary>,

        // your actual FTS columns; types depend on how you created it
        // change types to match your virtual table: Text/Binary/etc.
        node_id   -> Nullable<Integer>,
        content   -> Nullable<Text>,
    }
}

diesel::table! {
    code_fts_config (k) {
        k -> Binary,
        v -> Nullable<Binary>,
    }
}

diesel::table! {
    code_fts_content (id) {
        id -> Nullable<Integer>,
        c0 -> Nullable<Binary>,
        c1 -> Nullable<Binary>,
    }
}

diesel::table! {
    code_fts_data (id) {
        id -> Nullable<Integer>,
        block -> Nullable<Binary>,
    }
}

diesel::table! {
    code_fts_docsize (id) {
        id -> Nullable<Integer>,
        sz -> Nullable<Binary>,
    }
}

diesel::table! {
    code_fts_idx (segid, term) {
        segid -> Binary,
        term -> Binary,
        pgno -> Nullable<Binary>,
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

diesel::table! {
    file_content (node_id) {
        node_id -> Nullable<Integer>,
        blob_id -> Integer,
        line_count -> Nullable<Integer>,
        eol -> Nullable<Text>,
    }
}

diesel::table! {
    group_members (group_id, user_id) {
        group_id -> Text,
        user_id -> Text,
        role -> Text,
    }
}

diesel::table! {
    group_providers (group_id, provider_name) {
        group_id -> Text,
        provider_name -> Text,
    }
}

diesel::table! {
    groups (id) {
        id -> Nullable<Text>,
        name -> Text,
        description -> Text,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    logs (id) {
        id -> Nullable<Text>,
        user_id -> Nullable<Text>,
        category -> Nullable<Text>,
        subcategory -> Nullable<Text>,
        timestamp -> Timestamp,
        level -> Text,
        message -> Text,
    }
}

diesel::table! {
    node (id) {
        id -> Nullable<Integer>,
        project_id -> Integer,
        parent_id -> Nullable<Integer>,
        name -> Text,
        kind -> Text,
        is_symlink -> Integer,
        target_path -> Nullable<Text>,
        created_at -> Integer,
        updated_at -> Integer,
    }
}

diesel::table! {
    path_cache (node_id) {
        node_id -> Nullable<Integer>,
        abs_path -> Text,
    }
}

diesel::table! {
    plugin_file_content (node_id) {
        node_id -> Nullable<Integer>,
        blob_id -> Integer,
        line_count -> Nullable<Integer>,
        eol -> Nullable<Text>,
    }
}

diesel::table! {
    plugin_nodes (id) {
        id -> Nullable<Integer>,
        plugin_id -> Text,
        parent_id -> Nullable<Integer>,
        name -> Text,
        kind -> Text,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    plugin_path_cache (node_id) {
        node_id -> Nullable<Integer>,
        abs_path -> Text,
    }
}

diesel::table! {
    plugins (id) {
        id -> Nullable<Text>,
        project_id -> Text,
        owner_id -> Text,
        name -> Text,
        entry_path -> Text,
        runtime -> Text,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    project (id) {
        id -> Nullable<Integer>,
        name -> Text,
        created_at -> Integer,
    }
}

diesel::table! {
    project_providers (project_id, provider_name) {
        project_id -> Text,
        provider_name -> Text,
    }
}

diesel::table! {
    project_users (project_id, user_id) {
        project_id -> Text,
        user_id -> Text,
        role -> Text,
    }
}

diesel::table! {
    projects (id) {
        id -> Nullable<Text>,
        name -> Text,
        description -> Text,
        owner_id -> Text,
        visibility -> Text,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    user_providers (user_id, provider_name) {
        user_id -> Text,
        provider_name -> Text,
        source -> Text,
    }
}

diesel::joinable!(file_content -> blob (blob_id));
diesel::joinable!(file_content -> node (node_id));
diesel::joinable!(group_members -> auth (user_id));
diesel::joinable!(group_members -> groups (group_id));
diesel::joinable!(group_providers -> groups (group_id));
diesel::joinable!(node -> project (project_id));
diesel::joinable!(path_cache -> node (node_id));
diesel::joinable!(plugin_file_content -> blobs (blob_id));
diesel::joinable!(plugin_file_content -> plugin_nodes (node_id));
diesel::joinable!(plugin_nodes -> plugins (plugin_id));
diesel::joinable!(plugin_path_cache -> plugin_nodes (node_id));
diesel::joinable!(project_providers -> projects (project_id));
diesel::joinable!(project_users -> auth (user_id));
diesel::joinable!(project_users -> projects (project_id));
diesel::joinable!(projects -> auth (owner_id));
diesel::joinable!(user_providers -> auth (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    auth,
    blob,
    blobs,
    code_fts,
    code_fts_config,
    code_fts_content,
    code_fts_data,
    code_fts_docsize,
    code_fts_idx,
    entities,
    file_content,
    group_members,
    group_providers,
    groups,
    logs,
    node,
    path_cache,
    plugin_file_content,
    plugin_nodes,
    plugin_path_cache,
    plugins,
    project,
    project_providers,
    project_users,
    projects,
    user_providers,
);
