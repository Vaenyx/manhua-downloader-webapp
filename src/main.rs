use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        main {
            style: "
                min-height: 100vh;
                display: grid;
                place-content: center;
                padding: 2rem;
                text-align: center;
                font-family: system-ui, sans-serif;
                background: #f5f5f5;
                color: #181818;
            ",

            h1 {
                style: "font-size: 3rem; margin: 0;",
                "Hello from Rust!"
            }

            p {
                "This Dioxus application works offline."
            }
        }
    }
}
