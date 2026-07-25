use dioxus::prelude::*;

/// Ask the user to select a directory.
///
/// The directory handle is stored globally in the browser window and reused
/// by read_text_file, write_text_file, and list_files.
pub async fn select_folder() -> Result<String, String> {
    let mut eval = document::eval(
        r#"
        try {
            if (!window.showDirectoryPicker) {
                throw new Error(
                    "This browser does not support directory access."
                );
            }

            const directory = await window.showDirectoryPicker({
                id: "dioxus-application-folder",
                mode: "readwrite"
            });

            window.__dioxusFolderHandle = directory;

            dioxus.send(true);
            dioxus.send(directory.name);
        } catch (error) {
            const message =
                error?.name === "AbortError"
                    ? "Folder selection was cancelled."
                    : String(error?.message ?? error);

            dioxus.send(false);
            dioxus.send(message);
        }
        "#,
    );

    let success = eval
        .recv::<bool>()
        .await
        .map_err(|error| error.to_string())?;

    let message = eval
        .recv::<String>()
        .await
        .map_err(|error| error.to_string())?;

    if success {
        Ok(message)
    } else {
        Err(message)
    }
}

/// Write a UTF-8 text file into the selected directory.
///
/// An existing file with the same name will be overwritten.
pub async fn write_text_file(
    filename: &str,
    content: &str,
) -> Result<(), String> {
    validate_filename(filename)?;

    let mut eval = document::eval(
        r#"
        const filename = await dioxus.recv();
        const content = await dioxus.recv();

        try {
            const directory = window.__dioxusFolderHandle;

            if (!directory) {
                throw new Error("No folder has been selected.");
            }

            const fileHandle = await directory.getFileHandle(
                filename,
                { create: true }
            );

            const writable = await fileHandle.createWritable();

            await writable.write(content);
            await writable.close();

            dioxus.send(true);
            dioxus.send(`Saved "${filename}".`);
        } catch (error) {
            dioxus.send(false);
            dioxus.send(String(error?.message ?? error));
        }
        "#,
    );

    eval.send(filename)
        .map_err(|error| error.to_string())?;

    eval.send(content)
        .map_err(|error| error.to_string())?;

    let success = eval
        .recv::<bool>()
        .await
        .map_err(|error| error.to_string())?;

    let message = eval
        .recv::<String>()
        .await
        .map_err(|error| error.to_string())?;

    if success {
        Ok(())
    } else {
        Err(message)
    }
}

/// Read a UTF-8 text file from the selected directory.
pub async fn read_text_file(filename: &str) -> Result<String, String> {
    validate_filename(filename)?;

    let mut eval = document::eval(
        r#"
        const filename = await dioxus.recv();

        try {
            const directory = window.__dioxusFolderHandle;

            if (!directory) {
                throw new Error("No folder has been selected.");
            }

            const fileHandle = await directory.getFileHandle(
                filename,
                { create: false }
            );

            const file = await fileHandle.getFile();
            const content = await file.text();

            dioxus.send(true);
            dioxus.send(content);
        } catch (error) {
            dioxus.send(false);

            if (error?.name === "NotFoundError") {
                dioxus.send(`File "${filename}" does not exist.`);
            } else {
                dioxus.send(String(error?.message ?? error));
            }
        }
        "#,
    );

    eval.send(filename)
        .map_err(|error| error.to_string())?;

    let success = eval
        .recv::<bool>()
        .await
        .map_err(|error| error.to_string())?;

    let message = eval
        .recv::<String>()
        .await
        .map_err(|error| error.to_string())?;

    if success {
        Ok(message)
    } else {
        Err(message)
    }
}

/// Return the names of all files directly inside the selected directory.
///
/// Subdirectories are not included.
pub async fn list_files() -> Result<Vec<String>, String> {
    let mut eval = document::eval(
        r#"
        try {
            const directory = window.__dioxusFolderHandle;

            if (!directory) {
                throw new Error("No folder has been selected.");
            }

            const filenames = [];

            for await (const [name, handle] of directory.entries()) {
                if (handle.kind === "file") {
                    filenames.push(name);
                }
            }

            filenames.sort((left, right) =>
                left.localeCompare(right)
            );

            dioxus.send(true);
            dioxus.send(filenames);
        } catch (error) {
            dioxus.send(false);
            dioxus.send(String(error?.message ?? error));
        }
        "#,
    );

    let success = eval
        .recv::<bool>()
        .await
        .map_err(|error| error.to_string())?;

    if success {
        eval.recv::<Vec<String>>()
            .await
            .map_err(|error| error.to_string())
    } else {
        let message = eval
            .recv::<String>()
            .await
            .map_err(|error| error.to_string())?;

        Err(message)
    }
}

fn validate_filename(filename: &str) -> Result<(), String> {
    if filename.trim().is_empty() {
        return Err("The filename cannot be empty.".to_string());
    }

    if filename.contains('/') || filename.contains('\\') {
        return Err(
            "Enter only a filename, not a path. Slashes are not allowed."
                .to_string(),
        );
    }

    Ok(())
}

/// Blocks the rest of the application until the user selects a folder.
#[component]
pub fn FolderAccessGate(children: Element) -> Element {
    let mut folder_name = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut selecting = use_signal(|| false);
    let mut has_access = use_signal(|| false);

    rsx! {
        if has_access() {
            div {
                style: "
                    padding: 8px 16px;
                    background: #e8f5e9;
                    border-bottom: 1px solid #a5d6a7;
                    font-family: system-ui, sans-serif;
                ",

                strong { "Folder: " }
                "{folder_name}"
            }

            {children}
        } else {
            main {
                style: "
                    min-height: 100vh;
                    display: grid;
                    place-content: center;
                    padding: 2rem;
                    font-family: system-ui, sans-serif;
                    background: #f5f5f5;
                ",

                section {
                    style: "
                        width: min(420px, 90vw);
                        padding: 32px;
                        border-radius: 12px;
                        background: white;
                        box-shadow: 0 4px 20px rgba(0, 0, 0, 0.1);
                        text-align: center;
                    ",

                    h1 { "Select an application folder" }

                    p {
                        "The application needs permission to read and write files."
                    }

                    button {
                        disabled: selecting(),
                        style: "
                            padding: 10px 18px;
                            font: inherit;
                            cursor: pointer;
                        ",

                        onclick: move |_| async move {
                            selecting.set(true);
                            error.set(String::new());

                            match select_folder().await {
                                Ok(name) => {
                                    folder_name.set(name);
                                    has_access.set(true);
                                }
                                Err(message) => {
                                    error.set(message);
                                }
                            }

                            selecting.set(false);
                        },

                        if selecting() {
                            "Opening..."
                        } else {
                            "Select folder"
                        }
                    }

                    if !error().is_empty() {
                        p {
                            style: "color: #b00020;",
                            "{error}"
                        }
                    }
                }
            }
        }
    }
}
