use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

const FFLATE_CDN: &str = "https://cdn.jsdelivr.net/npm/fflate@0.8.2/umd/index.js";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChapterSummary {
    pub name: String,
    pub image_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NovelSummary {
    pub name: String,
    pub chapters: Vec<ChapterSummary>,
    pub favorite: bool,
    pub last_chapter: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct StorageInfo {
    name: String,
    mode: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ImportEvent {
    kind: String,
    message: String,
    current: usize,
    total: usize,
}

/// Let the user select a normal folder on the device.
///
/// This uses the File System Access API and therefore mainly works in
/// Chromium-based browsers in a secure context.
pub async fn select_folder() -> Result<String, String> {
    let mut eval = document::eval(
        r#"
        try {
            if (!window.showDirectoryPicker) {
                throw new Error(
                    "Folder selection is not supported by this browser. Use private app storage instead."
                );
            }

            const directory = await window.showDirectoryPicker({
                id: "dioxus-novel-library",
                mode: "readwrite"
            });

            window.__novelRootHandle = directory;

            dioxus.send(true);
            dioxus.send(directory.name);
        } catch (error) {
            const message = error?.name === "AbortError"
                ? "Folder selection was cancelled."
                : String(error?.message ?? error);

            dioxus.send(false);
            dioxus.send(message);
        }
        "#,
    );

    receive_result_string(&mut eval).await
}

/// Use the browser's Origin Private File System.
///
/// This is the mobile-friendly fallback. The files remain on the client, but
/// they are managed by the browser rather than shown as a normal device folder.
pub async fn use_private_storage() -> Result<String, String> {
    let mut eval = document::eval(
        r#"
        try {
            if (!navigator.storage?.getDirectory) {
                throw new Error("Private app storage is not supported by this browser.");
            }

            const storageRoot = await navigator.storage.getDirectory();
            const directory = await storageRoot.getDirectoryHandle(
                "novel-library",
                { create: true }
            );

            window.__novelRootHandle = directory;

            if (navigator.storage.persist) {
                try { await navigator.storage.persist(); } catch (_) {}
            }

            dioxus.send(true);
            dioxus.send("Private app storage");
        } catch (error) {
            dioxus.send(false);
            dioxus.send(String(error?.message ?? error));
        }
        "#,
    );

    receive_result_string(&mut eval).await
}

pub async fn list_novels() -> Result<Vec<NovelSummary>, String> {
    let mut eval = document::eval(
        r#"
        try {
            const root = window.__novelRootHandle;
            if (!root) throw new Error("No library folder is active.");

            const collator = new Intl.Collator(undefined, {
                numeric: true,
                sensitivity: "base"
            });

            const novels = [];

            for await (const [novelName, novelHandle] of root.entries()) {
                if (novelHandle.kind !== "directory") continue;

                const chapters = [];

                for await (const [chapterName, chapterHandle] of novelHandle.entries()) {
                    if (chapterHandle.kind !== "directory") continue;

                    let imageCount = 0;
                    for await (const [filename, fileHandle] of chapterHandle.entries()) {
                        if (
                            fileHandle.kind === "file" &&
                            filename.toLowerCase().endsWith(".webp")
                        ) {
                            imageCount += 1;
                        }
                    }

                    if (imageCount > 0) {
                        chapters.push({
                            name: chapterName,
                            image_count: imageCount
                        });
                    }
                }

                if (chapters.length === 0) continue;

                chapters.sort((a, b) => collator.compare(a.name, b.name));

                novels.push({
                    name: novelName,
                    chapters,
                    favorite: localStorage.getItem(`novel:favorite:${novelName}`) === "1",
                    last_chapter: localStorage.getItem(`novel:last:${novelName}`)
                });
            }

            novels.sort((a, b) => {
                if (a.favorite !== b.favorite) return a.favorite ? -1 : 1;
                return collator.compare(a.name, b.name);
            });

            dioxus.send(true);
            dioxus.send(novels);
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
        eval.recv::<Vec<NovelSummary>>()
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

/// Fetch a ZIP from the remote API and extract it into the active client folder.
///
/// Expected API query parameters:
/// - `url=<source chapter or novel URL>`
/// - `all_chapters=true|false`
///
/// Expected ZIP layout:
/// `chapter-name/1.webp`
///
/// The story name is taken from the ZIP filename supplied by the API in the
/// `Content-Disposition` response header.
pub async fn import_novel(
    api_endpoint: &str,
    source_url: &str,
    all_chapters: bool,
    mut progress: Signal<String>,
) -> Result<String, String> {
    let script = format!(
        r#"
        const endpoint = await dioxus.recv();
        const sourceUrl = await dioxus.recv();
        const allChapters = await dioxus.recv();
        const fflateCdn = {fflate_cdn:?};

        const send = (kind, message, current = 0, total = 0) =>
            dioxus.send({{ kind, message, current, total }});

        function safeParts(path) {{
            const normalized = path.replaceAll("\\", "/");
            const parts = normalized.split("/").filter(Boolean);

            if (
                parts.length === 0 ||
                parts.some(part => part === "." || part === ".." || part.includes("\0"))
            ) {{
                throw new Error(`Unsafe path in ZIP: ${{path}}`);
            }}

            return parts;
        }}

        function storyNameFromResponse(response) {{
            const disposition = response.headers.get("content-disposition") ?? "";
            let filename = "";

            const utf8Match = disposition.match(
                /filename\*\s*=\s*UTF-8''([^;]+)/i
            );

            if (utf8Match) {{
                try {{
                    filename = decodeURIComponent(
                        utf8Match[1].trim().replace(/^['"]|['"]$/g, "")
                    );
                }} catch (_) {{
                    filename = utf8Match[1].trim();
                }}
            }}

            if (!filename) {{
                const normalMatch = disposition.match(
                    /filename\s*=\s*(?:"([^"]+)"|([^;]+))/i
                );
                filename = normalMatch?.[1] ?? normalMatch?.[2]?.trim() ?? "";
            }}

            filename = filename.split(/[\\/]/).pop()?.trim() ?? "";
            const storyName = filename.replace(/\.zip$/i, "").trim();

            if (!storyName || storyName === "." || storyName === "..") {{
                throw new Error(
                    "The API did not expose a ZIP filename. Return " +
                    "Content-Disposition: attachment; filename=\"story-name.zip\" " +
                    "and expose Content-Disposition through CORS."
                );
            }}

            return storyName;
        }}

        async function ensureFflate() {{
            if (window.fflate) return;

            if (!window.__fflateLoading) {{
                window.__fflateLoading = new Promise((resolve, reject) => {{
                    const script = document.createElement("script");
                    script.src = fflateCdn;
                    script.async = true;
                    script.onload = resolve;
                    script.onerror = () => reject(
                        new Error("Could not load the ZIP library. Check the browser connection or vendor fflate locally.")
                    );
                    document.head.appendChild(script);
                }});
            }}

            await window.__fflateLoading;
            if (!window.fflate) throw new Error("The ZIP library did not initialize.");
        }}

        try {{
            const root = window.__novelRootHandle;
            if (!root) throw new Error("No library folder is active.");

            const apiUrl = new URL(endpoint.trim());
            apiUrl.searchParams.set("url", sourceUrl);
            apiUrl.searchParams.set("all_chapters", allChapters ? "true" : "false");

            send("progress", "Downloading ZIP from the API…");

            const response = await fetch(apiUrl, {{ method: "GET" }});
            if (!response.ok) {{
                let detail = "";
                try {{ detail = (await response.text()).slice(0, 300); }} catch (_) {{}}
                throw new Error(
                    `API returned HTTP ${{response.status}}${{detail ? `: ${{detail}}` : ""}}`
                );
            }}

            const storyName = storyNameFromResponse(response);
            const bytes = new Uint8Array(await response.arrayBuffer());
            if (bytes.length < 4 || bytes[0] !== 0x50 || bytes[1] !== 0x4b) {{
                throw new Error("The API response is not a ZIP file.");
            }}

            send("progress", "Extracting archive…");
            await ensureFflate();

            const archive = await new Promise((resolve, reject) => {{
                window.fflate.unzip(bytes, (error, data) =>
                    error ? reject(error) : resolve(data)
                );
            }});

            const files = [];

            for (const [path, data] of Object.entries(archive)) {{
                if (!path.toLowerCase().endsWith(".webp")) continue;

                const parts = safeParts(path);
                if (parts.length !== 2 && parts.length !== 3) {{
                    throw new Error(
                        `Expected chapter/image.webp, got: ${{path}}`
                    );
                }}

                // Prefer chapter/image.webp. A legacy wrapper/chapter/image.webp
                // layout is also accepted, but the wrapper is ignored because
                // the story name always comes from the ZIP filename.
                files.push({{
                    chapter: parts[parts.length - 2],
                    filename: parts[parts.length - 1],
                    data
                }});
            }}

            if (files.length === 0) {{
                throw new Error("The ZIP contains no chapter/*.webp images.");
            }}

            const storyHandle = await root.getDirectoryHandle(storyName, {{ create: true }});
            const chapterNames = [...new Set(files.map(file => file.chapter))];
            const chapterHandles = new Map();

            // Replace only chapters that are present in this ZIP. This prevents
            // stale image files when a chapter is re-imported with fewer pages.
            for (const chapterName of chapterNames) {{
                try {{
                    await storyHandle.removeEntry(chapterName, {{ recursive: true }});
                }} catch (error) {{
                    if (error?.name !== "NotFoundError") throw error;
                }}

                chapterHandles.set(
                    chapterName,
                    await storyHandle.getDirectoryHandle(chapterName, {{ create: true }})
                );
            }}

            const total = files.length;

            for (let index = 0; index < total; index += 1) {{
                const file = files[index];
                const chapterHandle = chapterHandles.get(file.chapter);
                const fileHandle = await chapterHandle.getFileHandle(
                    file.filename,
                    {{ create: true }}
                );
                const writable = await fileHandle.createWritable();
                await writable.write(file.data);
                await writable.close();

                if (index === 0 || index + 1 === total || (index + 1) % 5 === 0) {{
                    send(
                        "progress",
                        `Saving ${{index + 1}} of ${{total}} images…`,
                        index + 1,
                        total
                    );
                }}
            }}

            send(
                "done",
                storyName,
                total,
                total
            );
        }} catch (error) {{
            send("error", String(error?.message ?? error));
        }}
        "#,
        fflate_cdn = FFLATE_CDN,
    );

    let mut eval = document::eval(&script);

    eval.send(api_endpoint)
        .map_err(|error| error.to_string())?;
    eval.send(source_url)
        .map_err(|error| error.to_string())?;
    eval.send(all_chapters)
        .map_err(|error| error.to_string())?;

    loop {
        let event = eval
            .recv::<ImportEvent>()
            .await
            .map_err(|error| error.to_string())?;

        match event.kind.as_str() {
            "progress" => {
                if event.total > 0 {
                    progress.set(format!(
                        "{} ({}/{})",
                        event.message, event.current, event.total
                    ));
                } else {
                    progress.set(event.message);
                }
            }
            "done" => return Ok(event.message),
            "error" => return Err(event.message),
            _ => {}
        }
    }
}

/// Create temporary browser URLs for the images in one chapter.
/// The previous chapter's URLs are revoked before the new ones are created.
pub async fn load_chapter_images(
    novel_name: &str,
    chapter_name: &str,
) -> Result<Vec<String>, String> {
    let mut eval = document::eval(
        r#"
        const novelName = await dioxus.recv();
        const chapterName = await dioxus.recv();

        try {
            const root = window.__novelRootHandle;
            if (!root) throw new Error("No library folder is active.");

            for (const url of window.__novelObjectUrls ?? []) {
                URL.revokeObjectURL(url);
            }
            window.__novelObjectUrls = [];

            const novelHandle = await root.getDirectoryHandle(novelName, { create: false });
            const chapterHandle = await novelHandle.getDirectoryHandle(
                chapterName,
                { create: false }
            );

            const collator = new Intl.Collator(undefined, {
                numeric: true,
                sensitivity: "base"
            });

            const files = [];
            for await (const [filename, fileHandle] of chapterHandle.entries()) {
                if (
                    fileHandle.kind === "file" &&
                    filename.toLowerCase().endsWith(".webp")
                ) {
                    files.push({ filename, fileHandle });
                }
            }

            files.sort((a, b) => collator.compare(a.filename, b.filename));

            const urls = [];
            for (const item of files) {
                const file = await item.fileHandle.getFile();
                urls.push(URL.createObjectURL(file));
            }

            window.__novelObjectUrls = urls;
            localStorage.setItem(`novel:last:${novelName}`, chapterName);

            dioxus.send(true);
            dioxus.send(urls);
        } catch (error) {
            dioxus.send(false);
            dioxus.send(String(error?.message ?? error));
        }
        "#,
    );

    eval.send(novel_name)
        .map_err(|error| error.to_string())?;
    eval.send(chapter_name)
        .map_err(|error| error.to_string())?;

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

pub async fn release_image_urls() {
    let _ = document::eval(
        r#"
        for (const url of window.__novelObjectUrls ?? []) {
            URL.revokeObjectURL(url);
        }
        window.__novelObjectUrls = [];
        return true;
        "#,
    )
    .join::<bool>()
    .await;
}

pub async fn toggle_favorite(novel_name: &str) -> Result<bool, String> {
    let mut eval = document::eval(
        r#"
        const novelName = await dioxus.recv();
        try {
            const key = `novel:favorite:${novelName}`;
            const favorite = localStorage.getItem(key) !== "1";
            localStorage.setItem(key, favorite ? "1" : "0");
            dioxus.send(true);
            dioxus.send(favorite);
        } catch (error) {
            dioxus.send(false);
            dioxus.send(String(error?.message ?? error));
        }
        "#,
    );

    eval.send(novel_name)
        .map_err(|error| error.to_string())?;

    let success = eval
        .recv::<bool>()
        .await
        .map_err(|error| error.to_string())?;

    if success {
        eval.recv::<bool>()
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

pub async fn load_api_endpoint() -> Result<String, String> {
    document::eval(
        r#"
        return localStorage.getItem("novel:api-endpoint") ?? "";
        "#,
    )
    .join::<String>()
    .await
    .map_err(|error| error.to_string())
}

pub async fn save_api_endpoint(endpoint: &str) -> Result<(), String> {
    let mut eval = document::eval(
        r#"
        const endpoint = await dioxus.recv();
        localStorage.setItem("novel:api-endpoint", endpoint);
        return true;
        "#,
    );

    eval.send(endpoint)
        .map_err(|error| error.to_string())?;
    eval.join::<bool>()
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn scroll_to_top() {
    let _ = document::eval(
        "window.scrollTo({ top: 0, behavior: 'instant' }); return true;"
    )
    .join::<bool>()
    .await;
}

async fn receive_result_string(eval: &mut document::Eval) -> Result<String, String> {
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

/// Blocks the application until the user selects normal folder storage or the
/// browser-managed mobile fallback.
#[component]
pub fn FolderAccessGate(children: Element) -> Element {
    let mut storage_name = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut selecting = use_signal(|| false);
    let mut has_access = use_signal(|| false);

    rsx! {
        if has_access() {
            div { class: "storage-strip",
                span { class: "storage-dot" }
                span { "Library: {storage_name}" }
            }

            {children}
        } else {
            main { class: "gate-page",
                section { class: "gate-card",
                    div { class: "app-mark", "NS" }
                    h1 { "Choose your novel library" }
                    p { class: "muted",
                        "The app stores imported chapters locally on this device."
                    }

                    button {
                        class: "primary-button",
                        disabled: selecting(),
                        onclick: move |_| async move {
                            selecting.set(true);
                            error.set(String::new());

                            match select_folder().await {
                                Ok(name) => {
                                    storage_name.set(name);
                                    has_access.set(true);
                                }
                                Err(message) => error.set(message),
                            }

                            selecting.set(false);
                        },
                        if selecting() { "Opening…" } else { "Choose device folder" }
                    }

                    button {
                        class: "secondary-button",
                        disabled: selecting(),
                        onclick: move |_| async move {
                            selecting.set(true);
                            error.set(String::new());

                            match use_private_storage().await {
                                Ok(name) => {
                                    storage_name.set(name);
                                    has_access.set(true);
                                }
                                Err(message) => error.set(message),
                            }

                            selecting.set(false);
                        },
                        "Use private app storage"
                    }

                    p { class: "storage-help",
                        "Device folders work best in Chromium. Private storage is the mobile-friendly fallback."
                    }

                    if !error().is_empty() {
                        p { class: "error-banner", "{error}" }
                    }
                }
            }
        }
    }
}
