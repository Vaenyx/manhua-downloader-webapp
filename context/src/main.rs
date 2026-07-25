use dioxus::prelude::*;

mod folder_access;

use folder_access::{
    FolderAccessGate,
    list_files,
    read_text_file,
    write_text_file,
};

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        FolderAccessGate {
            FileEditor {}
        }
    }
}

#[component]
fn FileEditor() -> Element {
    let mut filename = use_signal(|| "hello.txt".to_string());
    let mut content = use_signal(|| "Hello from Dioxus!".to_string());
    let mut status = use_signal(String::new);
    let mut files = use_signal(Vec::<String>::new);
    let mut working = use_signal(|| false);

    rsx! {
        main {
            style: "
                max-width: 700px;
                margin: 30px auto;
                padding: 24px;
                font-family: system-ui, sans-serif;
            ",

            h1 { "File editor" }

            label {
                r#for: "filename",
                "Filename"
            }

            input {
                id: "filename",
                value: "{filename}",
                placeholder: "example.txt",
                style: "
                    display: block;
                    box-sizing: border-box;
                    width: 100%;
                    margin: 8px 0 20px;
                    padding: 10px;
                ",

                oninput: move |event| {
                    filename.set(event.value());
                },
            }

            label {
                r#for: "content",
                "Content"
            }

            textarea {
                id: "content",
                value: "{content}",
                rows: 12,
                style: "
                    display: block;
                    box-sizing: border-box;
                    width: 100%;
                    margin: 8px 0 20px;
                    padding: 10px;
                    resize: vertical;
                ",

                oninput: move |event| {
                    content.set(event.value());
                },
            }

            div {
                style: "
                    display: flex;
                    flex-wrap: wrap;
                    gap: 10px;
                ",

                button {
                    disabled: working(),

                    onclick: move |_| async move {
                        let name = filename();
                        let text = content();

                        working.set(true);
                        status.set("Saving...".to_string());

                        match write_text_file(&name, &text).await {
                            Ok(()) => {
                                status.set(format!("Saved \"{name}\"."));

                                if let Ok(updated_files) = list_files().await {
                                    files.set(updated_files);
                                }
                            }
                            Err(error) => {
                                status.set(format!("Save failed: {error}"));
                            }
                        }

                        working.set(false);
                    },

                    "Save"
                }

                button {
                    disabled: working(),

                    onclick: move |_| async move {
                        let name = filename();

                        working.set(true);
                        status.set("Reading...".to_string());

                        match read_text_file(&name).await {
                            Ok(text) => {
                                content.set(text);
                                status.set(format!("Loaded \"{name}\"."));
                            }
                            Err(error) => {
                                status.set(format!("Read failed: {error}"));
                            }
                        }

                        working.set(false);
                    },

                    "Read"
                }

                button {
                    disabled: working(),

                    onclick: move |_| async move {
                        working.set(true);

                        match list_files().await {
                            Ok(found_files) => {
                                let count = found_files.len();

                                files.set(found_files);
                                status.set(format!(
                                    "Found {count} file(s)."
                                ));
                            }
                            Err(error) => {
                                status.set(format!(
                                    "Could not list files: {error}"
                                ));
                            }
                        }

                        working.set(false);
                    },

                    "Refresh file list"
                }
            }

            if !status().is_empty() {
                p {
                    style: "margin-top: 20px;",
                    "{status}"
                }
            }

            h2 { "Files in selected folder" }

            if files().is_empty() {
                p { "No files loaded yet." }
            } else {
                ul {
                    for file in files() {
                        li {
                            button {
                                style: "
                                    padding: 3px 6px;
                                    border: none;
                                    background: none;
                                    text-decoration: underline;
                                    cursor: pointer;
                                ",

                                onclick: move |_| {
                                    filename.set(file.clone());
                                },

                                "{file}"
                            }
                        }
                    }
                }
            }
        }
    }
}
