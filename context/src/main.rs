use dioxus::prelude::*;

mod folder_access;

use folder_access::{
    FolderAccessGate, NovelSummary, import_novel,
    list_novels, load_api_endpoint, load_chapter_images,
    release_image_urls, save_api_endpoint, scroll_to_top,
    toggle_favorite,
};

static MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Title { "Novel Shelf" }
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover"
        }
        document::Meta { name: "theme-color", content: "#0b0d12" }
        document::Stylesheet { href: MAIN_CSS }

        FolderAccessGate {
            NovelApp {}
        }
    }
}

#[derive(Clone, PartialEq)]
enum View {
    Library,
    Import,
    Chapters(NovelSummary),
    Reader(ReaderState),
}

#[derive(Clone, PartialEq)]
struct ReaderState {
    novel: NovelSummary,
    chapter_index: usize,
    images: Vec<String>,
}

#[component]
fn NovelApp() -> Element {
    let mut view = use_signal(|| View::Library);
    let mut novels = use_signal(Vec::<NovelSummary>::new);
    let mut status = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut search = use_signal(String::new);

    let mut api_endpoint = use_signal(String::new);
    let mut source_url = use_signal(String::new);
    let mut all_chapters = use_signal(|| true);

    use_effect(move || {
        spawn(async move {
            if let Ok(saved) = load_api_endpoint().await {
                api_endpoint.set(saved);
            }

            refresh_library(novels, status, busy).await;
        });
    });

    let page = match view() {
        View::Library => rsx! {
            LibraryScreen {
                novels: novels(),
                search: search(),
                status: status(),
                busy: busy(),
                on_search: move |value| search.set(value),
                on_refresh: move |_| {
                    spawn(refresh_library(novels, status, busy));
                },
                on_import: move |_| {
                    status.set(String::new());
                    view.set(View::Import);
                },
                on_open: move |novel| {
                    status.set(String::new());
                    view.set(View::Chapters(novel));
                    spawn(scroll_to_top());
                },
                on_read: move |(novel, index)| {
                    open_chapter(
                        novel,
                        index,
                        view,
                        novels,
                        status,
                        busy,
                    );
                },
                on_toggle_favorite: move |name: String| {
                    spawn(async move {
                        match toggle_favorite(&name).await {
                            Ok(favorite) => {
                                let mut updated = novels();
                                if let Some(novel) = updated
                                    .iter_mut()
                                    .find(|novel| novel.name == name)
                                {
                                    novel.favorite = favorite;
                                }
                                updated.sort_by(|left, right| {
                                    right.favorite.cmp(&left.favorite).then_with(|| {
                                        left.name.to_lowercase().cmp(&right.name.to_lowercase())
                                    })
                                });
                                novels.set(updated);
                            }
                            Err(error) => status.set(format!(
                                "Could not update favorite: {error}"
                            )),
                        }
                    });
                },
            }
        },
        View::Import => rsx! {
            ImportScreen {
                api_endpoint: api_endpoint(),
                source_url: source_url(),
                all_chapters: all_chapters(),
                status: status(),
                busy: busy(),
                on_api_endpoint: move |value| api_endpoint.set(value),
                on_source_url: move |value| source_url.set(value),
                on_all_chapters: move |value| all_chapters.set(value),
                on_back: move |_| {
                    status.set(String::new());
                    view.set(View::Library);
                },
                on_submit: move |_| {
                    let endpoint = api_endpoint().trim().to_string();
                    let novel_url = source_url().trim().to_string();
                    let import_all = all_chapters();

                    if endpoint.is_empty() {
                        status.set("Enter your API endpoint.".to_string());
                        return;
                    }

                    if novel_url.is_empty() {
                        status.set("Enter the source novel or chapter URL.".to_string());
                        return;
                    }

                    spawn(async move {
                        busy.set(true);
                        status.set("Starting import…".to_string());

                        let _ = save_api_endpoint(&endpoint).await;

                        match import_novel(
                            &endpoint,
                            &novel_url,
                            import_all,
                            status,
                        )
                        .await
                        {
                            Ok(story_name) => {
                                match list_novels().await {
                                    Ok(found) => novels.set(found),
                                    Err(error) => {
                                        status.set(format!(
                                            "Imported {story_name}, but refresh failed: {error}"
                                        ));
                                        busy.set(false);
                                        return;
                                    }
                                }

                                source_url.set(String::new());
                                status.set(format!("Imported {story_name}."));
                                view.set(View::Library);
                                scroll_to_top().await;
                            }
                            Err(error) => {
                                status.set(format!("Import failed: {error}"));
                            }
                        }

                        busy.set(false);
                    });
                },
            }
        },
        View::Chapters(novel) => {
            let novel_for_read = novel.clone();

            rsx! {
                ChapterScreen {
                    novel,
                    status: status(),
                    busy: busy(),
                    on_back: move |_| {
                        view.set(View::Library);
                        spawn(refresh_library(novels, status, busy));
                    },
                    on_read: move |index| {
                        open_chapter(
                            novel_for_read.clone(),
                            index,
                            view,
                            novels,
                            status,
                            busy,
                        );
                    },
                }
            }
        }
        View::Reader(reader) => {
            let novel_for_back = reader.novel.clone();
            let novel_for_navigation = reader.novel.clone();

            rsx! {
                ReaderScreen {
                    reader,
                    busy: busy(),
                    on_back: move |_| {
                        let chapters = novel_for_back.clone();
                        spawn(async move {
                            release_image_urls().await;
                            view.set(View::Chapters(chapters));
                            scroll_to_top().await;
                        });
                    },
                    on_navigate: move |index| {
                        open_chapter(
                            novel_for_navigation.clone(),
                            index,
                            view,
                            novels,
                            status,
                            busy,
                        );
                    },
                }
            }
        }
    };

    rsx! {
        div { class: "app-shell", {page} }
    }
}

async fn refresh_library(
    mut novels: Signal<Vec<NovelSummary>>,
    mut status: Signal<String>,
    mut busy: Signal<bool>,
) {
    busy.set(true);
    status.set("Loading library…".to_string());

    match list_novels().await {
        Ok(found) => {
            let count = found.len();
            novels.set(found);
            status.set(if count == 0 {
                "Your library is empty. Import your first novel.".to_string()
            } else {
                String::new()
            });
        }
        Err(error) => status.set(format!("Could not load library: {error}")),
    }

    busy.set(false);
}

fn open_chapter(
    novel: NovelSummary,
    chapter_index: usize,
    mut view: Signal<View>,
    mut novels: Signal<Vec<NovelSummary>>,
    mut status: Signal<String>,
    mut busy: Signal<bool>,
) {
    let Some(chapter) = novel.chapters.get(chapter_index).cloned() else {
        status.set("That chapter no longer exists.".to_string());
        return;
    };

    spawn(async move {
        busy.set(true);
        status.set(format!("Loading {}…", chapter_label(&chapter.name)));

        match load_chapter_images(&novel.name, &chapter.name).await {
            Ok(images) if !images.is_empty() => {
                let mut updated_novel = novel.clone();
                updated_novel.last_chapter = Some(chapter.name.clone());

                let mut updated_library = novels();
                if let Some(item) = updated_library
                    .iter_mut()
                    .find(|item| item.name == updated_novel.name)
                {
                    item.last_chapter = Some(chapter.name.clone());
                }
                novels.set(updated_library);

                view.set(View::Reader(ReaderState {
                    novel: updated_novel,
                    chapter_index,
                    images,
                }));
                status.set(String::new());
                scroll_to_top().await;
            }
            Ok(_) => status.set("The chapter contains no WebP images.".to_string()),
            Err(error) => status.set(format!("Could not open chapter: {error}")),
        }

        busy.set(false);
    });
}

#[component]
fn LibraryScreen(
    novels: Vec<NovelSummary>,
    search: String,
    status: String,
    busy: bool,
    on_search: EventHandler<String>,
    on_refresh: EventHandler<()>,
    on_import: EventHandler<()>,
    on_open: EventHandler<NovelSummary>,
    on_read: EventHandler<(NovelSummary, usize)>,
    on_toggle_favorite: EventHandler<String>,
) -> Element {
    let needle = search.trim().to_lowercase();
    let visible = novels
        .into_iter()
        .filter(|novel| {
            needle.is_empty()
                || novel.name.to_lowercase().contains(&needle)
                || pretty_title(&novel.name).to_lowercase().contains(&needle)
        })
        .collect::<Vec<_>>();

    rsx! {
        header { class: "hero app-width",
            div {
                p { class: "eyebrow", "LOCAL NOVEL READER" }
                h1 { "Novel Shelf" }
                p { class: "hero-copy",
                    "Import from your scraper API and read directly from local client storage."
                }
            }
            button {
                class: "primary-button compact-button",
                disabled: busy,
                onclick: move |_| on_import.call(()),
                "+ Import"
            }
        }

        main { class: "app-width page-content",
            div { class: "toolbar",
                input {
                    class: "search-input",
                    value: search,
                    placeholder: "Search your library…",
                    oninput: move |event| on_search.call(event.value()),
                }
                button {
                    class: "secondary-button compact-button",
                    disabled: busy,
                    onclick: move |_| on_refresh.call(()),
                    if busy { "Loading…" } else { "Refresh" }
                }
            }

            StatusBanner { message: status }

            if visible.is_empty() {
                section { class: "empty-state",
                    h2 { "No novels found" }
                    p { class: "muted",
                        "Import a ZIP from your API or change the search text."
                    }
                    button {
                        class: "primary-button compact-button",
                        onclick: move |_| on_import.call(()),
                        "Import novel"
                    }
                }
            } else {
                section { class: "library-grid",
                    for novel in visible {
                        {
                            let title = pretty_title(&novel.name);
                            let chapter_count = novel.chapters.len();
                            let chapter_count_label = format!(
                                "{chapter_count} chapter{}",
                                if chapter_count == 1 { "" } else { "s" }
                            );
                            let open_novel = novel.clone();
                            let favorite_name = novel.name.clone();
                            let continue_index = novel.last_chapter.as_ref().and_then(|last| {
                                novel.chapters.iter().position(|chapter| &chapter.name == last)
                            });

                            rsx! {
                                article { class: "novel-card",
                                    div { class: "novel-card-top",
                                        button {
                                            class: "title-button",
                                            onclick: move |_| on_open.call(open_novel.clone()),
                                            h2 { "{title}" }
                                        }
                                        button {
                                            class: "favorite-button",
                                            aria_label: "Toggle favorite",
                                            onclick: move |_| {
                                                on_toggle_favorite.call(favorite_name.clone())
                                            },
                                            if novel.favorite { "★" } else { "☆" }
                                        }
                                    }

                                    p { class: "muted", "{chapter_count_label}" }

                                    div { class: "card-actions",
                                        button {
                                            class: "secondary-button compact-button",
                                            onclick: move |_| on_open.call(novel.clone()),
                                            "Chapters"
                                        }

                                        if let Some(index) = continue_index {
                                            {
                                                let continue_novel = novel.clone();
                                                rsx! {
                                                    button {
                                                        class: "primary-button compact-button",
                                                        onclick: move |_| {
                                                            on_read.call((continue_novel.clone(), index))
                                                        },
                                                        "Continue"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        BottomNav {
            library_active: true,
            on_library: move |_| {},
            on_import: move |_| on_import.call(()),
        }
    }
}

#[component]
fn ImportScreen(
    api_endpoint: String,
    source_url: String,
    all_chapters: bool,
    status: String,
    busy: bool,
    on_api_endpoint: EventHandler<String>,
    on_source_url: EventHandler<String>,
    on_all_chapters: EventHandler<bool>,
    on_back: EventHandler<()>,
    on_submit: EventHandler<()>,
) -> Element {
    rsx! {
        header { class: "topbar",
            div { class: "app-width topbar-inner",
                button {
                    class: "text-button",
                    disabled: busy,
                    onclick: move |_| on_back.call(()),
                    "← Library"
                }
                h1 { "Import novel" }
                span { class: "topbar-spacer" }
            }
        }

        main { class: "app-width page-content narrow-content",
            section { class: "form-card",
                label { r#for: "api-endpoint", "API endpoint" }
                input {
                    id: "api-endpoint",
                    value: api_endpoint,
                    placeholder: "https://api.example.com/download",
                    inputmode: "url",
                    oninput: move |event| on_api_endpoint.call(event.value()),
                }
                p { class: "field-help",
                    "The app appends ?url=…&all_chapters=true|false."
                }

                label { r#for: "source-url", "Novel or chapter URL" }
                input {
                    id: "source-url",
                    value: source_url,
                    placeholder: "https://source.example/manga/story/chapter-1",
                    inputmode: "url",
                    oninput: move |event| on_source_url.call(event.value()),
                }

                label { class: "check-row",
                    input {
                        r#type: "checkbox",
                        checked: all_chapters,
                        onchange: move |event| on_all_chapters.call(event.checked()),
                    }
                    span {
                        strong { "Import all chapters" }
                        small { "Disable this to request only the supplied chapter." }
                    }
                }

                button {
                    class: "primary-button",
                    disabled: busy,
                    onclick: move |_| on_submit.call(()),
                    if busy { "Importing…" } else { "Download and save" }
                }

                StatusBanner { message: status }
            }

            section { class: "info-card",
                h2 { "Required ZIP layout" }
                code { "story-name/chapter-name/1.webp" }
                code { "story-name/chapter-name/2.webp" }
                p { class: "muted",
                    "Image files are naturally sorted, so 2.webp appears before 10.webp. Existing chapters included in an update are replaced cleanly."
                }
            }
        }

        BottomNav {
            library_active: false,
            on_library: move |_| on_back.call(()),
            on_import: move |_| {},
        }
    }
}

#[component]
fn ChapterScreen(
    novel: NovelSummary,
    status: String,
    busy: bool,
    on_back: EventHandler<()>,
    on_read: EventHandler<usize>,
) -> Element {
    let title = pretty_title(&novel.name);
    let last = novel.last_chapter.clone();

    rsx! {
        header { class: "topbar",
            div { class: "app-width topbar-inner",
                button {
                    class: "text-button",
                    disabled: busy,
                    onclick: move |_| on_back.call(()),
                    "← Library"
                }
                h1 { "{title}" }
                span { class: "topbar-spacer" }
            }
        }

        main { class: "app-width page-content narrow-content",
            StatusBanner { message: status }

            if let Some(last_chapter) = last.clone() {
                if let Some(index) = novel
                    .chapters
                    .iter()
                    .position(|chapter| chapter.name == last_chapter)
                {
                    {
                        let continue_label = chapter_label(&last_chapter);
                        rsx! {
                    button {
                        class: "continue-card",
                        disabled: busy,
                        onclick: move |_| on_read.call(index),
                        span { "Continue reading" }
                        strong { "{continue_label} →" }
                    }
                        }
                    }
                }
            }

            section { class: "chapter-list",
                for (index, chapter) in novel.chapters.iter().cloned().enumerate() {
                    {
                        let label = chapter_label(&chapter.name);
                        let image_label = format!(
                            "{} image{}",
                            chapter.image_count,
                            if chapter.image_count == 1 { "" } else { "s" }
                        );
                        rsx! {
                    button {
                        class: if Some(chapter.name.clone()) == last {
                            "chapter-row current-chapter"
                        } else {
                            "chapter-row"
                        },
                        disabled: busy,
                        onclick: move |_| on_read.call(index),
                        span {
                            strong { "{label}" }
                            small { "{image_label}" }
                        }
                        span { class: "chevron", "›" }
                    }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ReaderScreen(
    reader: ReaderState,
    busy: bool,
    on_back: EventHandler<()>,
    on_navigate: EventHandler<usize>,
) -> Element {
    let mut zoom = use_signal(|| 100_u16);
    let chapter = &reader.novel.chapters[reader.chapter_index];
    let chapter_title = chapter_label(&chapter.name);
    let title = pretty_title(&reader.novel.name);
    let previous = reader.chapter_index.checked_sub(1);
    let next = (reader.chapter_index + 1 < reader.novel.chapters.len())
        .then_some(reader.chapter_index + 1);

    rsx! {
        header { class: "reader-topbar",
            button {
                class: "text-button",
                disabled: busy,
                onclick: move |_| on_back.call(()),
                "← Chapters"
            }
            div {
                strong { "{title}" }
                small { "{chapter_title}" }
            }
        }

        main { class: "reader-page",
            div { class: "zoom-bar app-width",
                label { r#for: "zoom", "Width" }
                input {
                    id: "zoom",
                    r#type: "range",
                    min: "35",
                    max: "100",
                    value: "{zoom}",
                    oninput: move |event| {
                        if let Ok(value) = event.value().parse::<u16>() {
                            zoom.set(value);
                        }
                    },
                }
                output { "{zoom}%" }
            }

            section { class: "reader-images",
                for (index, source) in reader.images.iter().cloned().enumerate() {
                    {
                        let page_number = index + 1;
                        rsx! {
                    img {
                        key: "{source}",
                        class: "reader-image",
                        src: source,
                        alt: "Page {page_number}",
                        loading: if index < 2 { "eager" } else { "lazy" },
                        decoding: "async",
                        style: "width: {zoom}%;",
                    }
                        }
                    }
                }
            }

            nav { class: "reader-nav app-width",
                button {
                    class: "secondary-button",
                    disabled: previous.is_none() || busy,
                    onclick: move |_| {
                        if let Some(index) = previous {
                            on_navigate.call(index);
                        }
                    },
                    "← Previous"
                }
                button {
                    class: "primary-button",
                    disabled: next.is_none() || busy,
                    onclick: move |_| {
                        if let Some(index) = next {
                            on_navigate.call(index);
                        }
                    },
                    "Next →"
                }
            }
        }
    }
}

#[component]
fn StatusBanner(message: String) -> Element {
    if message.is_empty() {
        return rsx! {};
    }

    rsx! {
        p { class: "status-banner", "{message}" }
    }
}

#[component]
fn BottomNav(
    library_active: bool,
    on_library: EventHandler<()>,
    on_import: EventHandler<()>,
) -> Element {
    rsx! {
        nav { class: "bottom-nav",
            div { class: "app-width bottom-nav-inner",
                button {
                    class: if library_active { "active" } else { "" },
                    onclick: move |_| on_library.call(()),
                    span { "▦" }
                    small { "Library" }
                }
                button {
                    class: if library_active { "" } else { "active" },
                    onclick: move |_| on_import.call(()),
                    span { "+" }
                    small { "Import" }
                }
            }
        }
    }
}

fn pretty_title(value: &str) -> String {
    value
        .replace('-', " ").replace('_', " ")
        .split_whitespace()
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + characters.as_str()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn chapter_label(value: &str) -> String {
    let lower = value.to_lowercase();

    for prefix in ["chapter-", "chapter_", "chapter "] {
        if lower.starts_with(prefix) {
            return format!("Chapter {}", &value[prefix.len()..]);
        }
    }

    if lower.starts_with("ch-") || lower.starts_with("ch_") {
        return format!("Chapter {}", &value[3..]);
    }

    format!("Chapter {value}")
}
