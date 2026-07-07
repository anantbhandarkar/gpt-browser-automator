//! gptbrowser — send a prompt to a logged-in chatbot web UI via PinchTab and
//! return the clean text response, deterministically. Feels like an API call.
//!
//! Usage:
//!   gptbrowser <url> <prompt...>
//!   gptbrowser --url <url> --prompt "<text>"
//!   echo "<prompt>" | gptbrowser <url>

use serde_json::{json, Value};
use std::io::Read;
use std::time::{Duration, Instant};

// ── TUNE HERE — the completion detector ──────────────────────────────────────
const DEFAULT_POLL_MS: u64 = 700; // how often we sample response length
const DEFAULT_STABLE_POLLS: u64 = 6; // no-growth samples in a row => "done" (~4.2s)
const DEFAULT_TOTAL_TIMEOUT_S: u64 = 240; // hard ceiling on a single generation
const FIRST_TOKEN_TIMEOUT_S: u64 = 45; // how long we wait for generation to start
const COMPOSER_TIMEOUT_S: u64 = 30; // how long we wait for the page/composer to load
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum Site {
    ChatGpt,
    Gemini,
    Kimi,
    DeepSeek,
    Claude,
    Perplexity,
    Copilot,
    Grok,
    Zai,
}

impl Site {
    fn detect(url: &str) -> Option<Site> {
        let u = url.to_lowercase();
        if u.contains("chatgpt") || u.contains("chat.openai") {
            Some(Site::ChatGpt)
        } else if u.contains("gemini") {
            Some(Site::Gemini)
        } else if u.contains("kimi") {
            Some(Site::Kimi)
        } else if u.contains("deepseek") {
            Some(Site::DeepSeek)
        } else if u.contains("claude") {
            Some(Site::Claude)
        } else if u.contains("perplexity") {
            Some(Site::Perplexity)
        } else if u.contains("copilot") {
            Some(Site::Copilot)
        } else if u.contains("grok") {
            Some(Site::Grok)
        } else if u.contains("z.ai") {
            Some(Site::Zai)
        } else {
            None
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Site::ChatGpt => "chatgpt",
            Site::Gemini => "gemini",
            Site::Kimi => "kimi",
            Site::DeepSeek => "deepseek",
            Site::Claude => "claude",
            Site::Perplexity => "perplexity",
            Site::Copilot => "copilot",
            Site::Grok => "grok",
            Site::Zai => "zai",
        }
    }

    /// Canonical app URL to navigate to. We detect the site from whatever the
    /// user typed, but always drive the real chat app — e.g. `gemini.com` is the
    /// crypto exchange; the AI lives at gemini.google.com.
    fn app_url(&self) -> &'static str {
        match self {
            Site::ChatGpt => "https://chatgpt.com/",
            Site::Gemini => "https://gemini.google.com/app",
            Site::Kimi => "https://www.kimi.com/",
            Site::DeepSeek => "https://chat.deepseek.com/",
            Site::Claude => "https://claude.ai/new",
            Site::Perplexity => "https://www.perplexity.ai/",
            Site::Copilot => "https://copilot.microsoft.com/",
            Site::Grok => "https://grok.com/",
            Site::Zai => "https://chat.z.ai/",
        }
    }

    fn composer_present_js(&self) -> &'static str {
        match self {
            Site::ChatGpt => "!!document.querySelector('#prompt-textarea')",
            Site::Gemini => "!!document.querySelector('.ql-editor')",
            Site::Kimi => "!!document.querySelector('.chat-input-editor')",
            Site::DeepSeek => "!!document.querySelector('#chat-input, textarea')",
            Site::Claude => "!!document.querySelector('div[data-testid=\"chat-input\"]')",
            Site::Perplexity => "!!document.querySelector('#ask-input')",
            Site::Copilot => "!!document.querySelector('textarea')",
            Site::Grok => "!!document.querySelector('textarea')",
            Site::Zai => "!!document.querySelector('#chat-input, textarea')",
        }
    }

    /// JS IIFE returning {ok, len, err}. Injects `lit` (a JS string literal).
    /// Kimi's Lexical editor won't accept a Range-based insertText until a real
    /// pointer interaction initializes its internal selection — so click first.
    fn needs_click_activation(&self) -> bool {
        matches!(self, Site::Kimi)
    }

    /// Sites whose composer is a real <textarea> (vs a contenteditable editor).
    fn is_textarea(&self) -> bool {
        matches!(self, Site::DeepSeek | Site::Zai | Site::Grok | Site::Copilot)
    }

    /// Primary CSS selector for the composer element.
    fn composer_sel(&self) -> &'static str {
        match self {
            Site::ChatGpt => "#prompt-textarea",
            Site::Gemini => ".ql-editor",
            Site::Kimi => ".chat-input-editor",
            Site::DeepSeek => "#chat-input",
            Site::Claude => "div[data-testid=\"chat-input\"]",
            Site::Perplexity => "#ask-input",
            Site::Copilot => "textarea",
            Site::Grok => "textarea",
            Site::Zai => "#chat-input",
        }
    }

    /// JS IIFE returning {ok, len}. `len` is the whitespace-stripped length of
    /// the composer after injection — the caller checks it actually landed.
    fn inject_js(&self, lit: &str) -> String {
        let sel = self.composer_sel();
        if self.is_textarea() {
            // Real <textarea> — React-safe native value setter + input event.
            return format!(
                "(function(){{var el=document.querySelector('{sel}')||document.querySelector('textarea');\
                 if(!el)return {{ok:false,err:'composer not found'}};el.focus();\
                 var s=Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype,'value').set;\
                 s.call(el,{lit});el.dispatchEvent(new Event('input',{{bubbles:true}}));\
                 return {{ok:true,len:(el.value||'').replace(/\\s+/g,'').length}};}})()"
            );
        }
        if matches!(self, Site::Kimi | Site::Perplexity) {
            // Lexical (Kimi, Perplexity): execCommand('delete') wipes the selection
            // and makes insertText no-op. Select all contents via a Range, then
            // insertText REPLACES it — this also overwrites Perplexity's persisted draft.
            return format!(
                "(function(){{var el=document.querySelector('{sel}');\
                 if(!el)return {{ok:false,err:'composer not found'}};el.focus();\
                 var r=document.createRange();r.selectNodeContents(el);\
                 var s=getSelection();s.removeAllRanges();s.addRange(r);\
                 document.execCommand('insertText',false,{lit});\
                 return {{ok:true,len:(el.innerText||el.textContent||'').replace(/\\s+/g,'').length}};}})()"
            );
        }
        // ProseMirror/Quill/tiptap (ChatGPT, Gemini, Claude): selectAll+delete then insertText.
        format!(
            "(function(){{var el=document.querySelector('{sel}');\
             if(!el)return {{ok:false,err:'composer not found'}};el.focus();\
             try{{document.execCommand('selectAll',false,null);document.execCommand('delete',false,null);}}catch(e){{}}\
             document.execCommand('insertText',false,{lit});\
             return {{ok:true,len:(el.innerText||el.textContent||'').replace(/\\s+/g,'').length}};}})()"
        )
    }

    /// Send button selector, or None for sites where we submit via Enter.
    fn send_button_sel(&self) -> Option<&'static str> {
        match self {
            Site::ChatGpt => Some("button[data-testid=\"send-button\"]"),
            Site::Gemini => Some("button[aria-label=\"Send message\"]"),
            Site::Claude => Some("button[aria-label=\"Send message\"]"),
            Site::Perplexity => Some("button[aria-label=\"Submit\"]"),
            Site::Zai => Some("#send-message-button"),
            Site::Grok => Some("button[data-testid=\"chat-submit\"]"),
            // Kimi's div-button + DeepSeek/Copilot: Enter-dispatch is more reliable.
            Site::Kimi | Site::DeepSeek | Site::Copilot => None,
        }
    }

    /// JS IIFE that dispatches a synthetic Enter keydown on the composer to
    /// submit. Reliable for textarea + Lexical where a CDP key press isn't.
    fn enter_submit_js(&self) -> String {
        let sel = if self.is_textarea() {
            format!("document.querySelector('{}')||document.querySelector('textarea')", self.composer_sel())
        } else {
            format!("document.querySelector('{}')", self.composer_sel())
        };
        format!(
            "(function(){{var el={sel};if(!el)return {{ok:false}};el.focus();\
             ['keydown','keypress','keyup'].forEach(function(t){{el.dispatchEvent(new KeyboardEvent(t,{{key:'Enter',code:'Enter',keyCode:13,which:13,bubbles:true,cancelable:true}}));}});\
             return {{ok:true}};}})()"
        )
    }

    /// JS IIFE -> latest assistant message text (HTML-free innerText).
    fn response_js(&self) -> &'static str {
        match self {
            Site::ChatGpt => r#"(function(){var n=document.querySelectorAll('[data-message-author-role="assistant"]');if(!n.length)return '';var e=n[n.length-1];var m=e.querySelector('.markdown');return ((m||e).innerText||'').trim();})()"#,
            Site::Gemini => r#"(function(){var ss=['.model-response-text','message-content','.markdown'];for(var i=0;i<ss.length;i++){var n=document.querySelectorAll(ss[i]);if(n.length)return (n[n.length-1].innerText||'').trim();}return '';})()"#,
            Site::Kimi => r#"(function(){var ss=['.segment-assistant .markdown','.segment-assistant'];for(var i=0;i<ss.length;i++){var n=document.querySelectorAll(ss[i]);if(n.length)return (n[n.length-1].innerText||'').trim();}return '';})()"#,
            Site::DeepSeek => r#"(function(){var ss=['.ds-markdown','[class*="markdown"]'];for(var i=0;i<ss.length;i++){var n=document.querySelectorAll(ss[i]);if(n.length)return (n[n.length-1].innerText||'').trim();}return '';})()"#,
            Site::Claude => r#"(function(){var n=document.querySelectorAll('.standard-markdown');if(!n.length){n=document.querySelectorAll('[data-is-streaming]');}if(!n.length)return '';var t=(n[n.length-1].innerText||'').trim();return t.replace(/^Claude responded:\s*/,'').trim();})()"#,
            Site::Perplexity => r#"(function(){var n=document.querySelectorAll('.prose');if(!n.length)return '';return (n[n.length-1].innerText||'').trim();})()"#,
            Site::Zai => r#"(function(){var n=document.querySelectorAll('.chat-assistant.markdown-prose');if(!n.length)return '';var t=(n[n.length-1].innerText||'').trim();t=t.replace(/^Thought Process\s*/i,'').trim();if(/^Thinking/i.test(t)||t==='Skip'||t==='')return '';return t;})()"#,
            Site::Grok => r#"(function(){var ss=['.message-bubble','[class*="markdown"]','.prose'];for(var i=0;i<ss.length;i++){var n=document.querySelectorAll(ss[i]);if(n.length)return (n[n.length-1].innerText||'').trim();}return '';})()"#,
            Site::Copilot => r#"(function(){var ss=['[data-content="ai-message"]','.ac-textBlock','[class*="message"]'];for(var i=0;i<ss.length;i++){var n=document.querySelectorAll(ss[i]);if(n.length)return (n[n.length-1].innerText||'').trim();}return '';})()"#,
        }
    }

    /// JS boolean: currently generating? Used ONLY to *block* premature "done".
    fn generating_js(&self) -> &'static str {
        match self {
            Site::ChatGpt => "!!document.querySelector('button[data-testid=\"stop-button\"]')",
            Site::Claude => "!!document.querySelector('button[aria-label*=\"Stop\" i]')",
            _ => "false",
        }
    }

    /// Selector for the control that opens the model picker. None where there is
    /// no picker (DeepSeek uses toggles) or the site isn't switch-capable here.
    fn model_picker_open_sel(&self) -> Option<&'static str> {
        match self {
            Site::ChatGpt => Some("[data-testid=\"model-switcher-dropdown-button\"]"),
            Site::Gemini => Some("button[aria-label*=\"Open mode picker\"]"),
            Site::Kimi => Some(".chat-editor-action .current-model"),
            Site::Claude => Some("[data-testid=\"model-selector-dropdown\"]"),
            Site::Perplexity => Some("button[aria-label=\"Model\"]"),
            Site::Zai => Some("button[aria-label=\"Select a model\"]"),
            Site::DeepSeek | Site::Copilot | Site::Grok => None,
        }
    }

    /// Comma-separated selector(s) matching model options in the open picker.
    fn model_option_sel(&self) -> &'static str {
        match self {
            Site::ChatGpt => "[role=menuitem],[role=menuitemradio]",
            Site::Gemini => "[role=menuitem]",
            Site::Kimi => ".model-item",
            Site::Claude => "[role=menuitemradio]",
            Site::Perplexity => "[role=menuitem]",
            Site::Zai => "[role=option],[role=menuitem],li",
            _ => "",
        }
    }

    /// Some sites hide extra models behind an expander (Claude's "More models").
    fn model_expand_text(&self) -> Option<&'static str> {
        match self {
            Site::Claude => Some("More models"),
            _ => None,
        }
    }

    /// Whether `--image` mode is supported (only ChatGPT for now).
    fn supports_image(&self) -> bool {
        matches!(self, Site::ChatGpt)
    }

    /// JS IIFE returning the src of the latest generated image, or '' if none yet.
    fn image_src_js(&self) -> &'static str {
        r#"(function(){var out='';document.querySelectorAll('[data-message-author-role="assistant"] img, main img').forEach(function(im){var s=im.src||im.currentSrc||'';if(/oaiusercontent|sdmnt|files\.oai|dalle|blob:/i.test(s)&&(im.naturalWidth>150||im.width>150))out=s;});return out;})()"#
    }
}

struct Client {
    agent: ureq::Agent,
    base: String,
    token: String,
}

impl Client {
    fn new(base: String, token: String) -> Client {
        let agent = ureq::AgentBuilder::new()
            .timeout_read(Duration::from_secs(90))
            .timeout_write(Duration::from_secs(30))
            .build();
        Client { agent, base, token }
    }

    fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Content-Type", "application/json")
            .send_string(&body.to_string());
        parse(resp)
    }

    fn get(&self, path: &str) -> Result<Value, String> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .call();
        parse(resp)
    }

    fn run_js(&self, tab: &str, expr: &str) -> Result<Value, String> {
        let v = self.post(&format!("/tabs/{tab}/evaluate"), json!({ "expression": expr }))?;
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    }

    /// GET returning the raw response body bytes (for binary downloads).
    fn get_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .call();
        match resp {
            Ok(r) => {
                let mut buf = Vec::new();
                r.into_reader()
                    .read_to_end(&mut buf)
                    .map_err(|e| format!("read bytes: {e}"))?;
                Ok(buf)
            }
            Err(ureq::Error::Status(code, r)) => {
                Err(format!("HTTP {code}: {}", r.into_string().unwrap_or_default()))
            }
            Err(e) => Err(format!("request failed: {e}")),
        }
    }

    fn js_true(&self, tab: &str, expr: &str) -> bool {
        matches!(self.run_js(tab, expr), Ok(Value::Bool(true)))
    }

    fn focus(&self, tab: &str) {
        let _ = self.post("/tab", json!({ "action": "focus", "tabId": tab }));
    }
}

fn parse(resp: Result<ureq::Response, ureq::Error>) -> Result<Value, String> {
    match resp {
        Ok(r) => {
            let s = r.into_string().map_err(|e| format!("read body: {e}"))?;
            serde_json::from_str(&s).map_err(|e| format!("bad json ({e}): {s}"))
        }
        Err(ureq::Error::Status(code, r)) => {
            let s = r.into_string().unwrap_or_default();
            Err(format!("HTTP {code}: {s}"))
        }
        Err(e) => Err(format!("request failed: {e} (is the PinchTab daemon running?)")),
    }
}

fn load_config() -> Result<(String, String), String> {
    if let (Ok(url), Ok(tok)) = (std::env::var("PINCHTAB_URL"), std::env::var("PINCHTAB_TOKEN")) {
        return Ok((url.trim_end_matches('/').to_string(), tok));
    }
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let path = format!("{home}/.pinchtab/config.json");
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let cfg: Value = serde_json::from_str(&raw).map_err(|e| format!("bad config json: {e}"))?;
    let server = cfg.get("server").cloned().unwrap_or(Value::Null);
    let port = server.get("port").and_then(|v| v.as_str()).unwrap_or("").trim();
    let port = if port.is_empty() { "9867" } else { port };
    let token = server
        .get("token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("no server.token in ~/.pinchtab/config.json")?
        .to_string();
    let base = std::env::var("PINCHTAB_URL")
        .map(|u| u.trim_end_matches('/').to_string())
        .unwrap_or_else(|_| format!("http://localhost:{port}"));
    Ok((base, token))
}

struct Args {
    url: String,
    prompt: String,
    new_tab: bool,
    poll_ms: u64,
    stable: u64,
    timeout_s: u64,
    model: Option<String>,
    list_models: bool,
    image: bool,
    out: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut url = String::new();
    let mut prompt_parts: Vec<String> = Vec::new();
    let mut new_tab = false;
    let mut poll_ms = DEFAULT_POLL_MS;
    let mut stable = DEFAULT_STABLE_POLLS;
    let mut timeout_s = DEFAULT_TOTAL_TIMEOUT_S;
    let mut model: Option<String> = None;
    let mut list_models = false;
    let mut image = false;
    let mut out: Option<String> = None;

    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        match a.as_str() {
            "--json" => {} // handled in main() via env scan
            "--new-tab" => new_tab = true,
            "--model" => { i += 1; model = Some(argv.get(i).cloned().ok_or("--model needs a value")?); }
            "--list-models" => list_models = true,
            "--image" => image = true,
            "--out" => { i += 1; out = Some(argv.get(i).cloned().ok_or("--out needs a value")?); }
            "--url" => { i += 1; url = argv.get(i).cloned().ok_or("--url needs a value")?; }
            "--prompt" => { i += 1; prompt_parts.push(argv.get(i).cloned().ok_or("--prompt needs a value")?); }
            "--poll" => { i += 1; poll_ms = argv.get(i).and_then(|v| v.parse().ok()).ok_or("--poll needs a number")?; }
            "--stable" => { i += 1; stable = argv.get(i).and_then(|v| v.parse().ok()).ok_or("--stable needs a number")?; }
            "--timeout" => { i += 1; timeout_s = argv.get(i).and_then(|v| v.parse().ok()).ok_or("--timeout needs a number")?; }
            "-h" | "--help" => return Err("HELP".to_string()),
            _ if url.is_empty() && !a.starts_with('-') => url = a.clone(),
            _ => prompt_parts.push(a.clone()),
        }
        i += 1;
    }

    if url.is_empty() {
        return Err("no URL given".to_string());
    }
    let mut prompt = prompt_parts.join(" ").trim().to_string();
    if prompt.is_empty() {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf).ok();
        prompt = buf.trim().to_string();
    }
    if prompt.is_empty() && !list_models {
        return Err("no prompt given (pass as args or via stdin)".to_string());
    }
    Ok(Args { url, prompt, new_tab, poll_ms, stable, timeout_s, model, list_models, image, out })
}

fn normalize_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

fn acquire_tab(c: &Client, site: Site, url: &str, new_tab: bool) -> Result<String, String> {
    if !new_tab {
        if let Ok(Value::Array(tabs)) = c.get("/tabs") {
            for t in &tabs {
                let turl = t.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let ttype = t.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if ttype == "page" && Site::detect(turl) == Some(site) {
                    if let Some(id) = t.get("id").and_then(|v| v.as_str()) {
                        return Ok(id.to_string());
                    }
                }
            }
        }
    }
    let v = c.post("/tab", json!({ "action": "new", "url": url }))?;
    let id = v.get("tabId").or_else(|| v.get("id")).and_then(|x| x.as_str()).map(|s| s.to_string());
    if let Some(id) = id {
        return Ok(id);
    }
    if let Ok(Value::Array(tabs)) = c.get("/tabs") {
        for t in &tabs {
            let turl = t.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if Site::detect(turl) == Some(site) {
                if let Some(id) = t.get("id").and_then(|v| v.as_str()) {
                    return Ok(id.to_string());
                }
            }
        }
    }
    Err("could not create or find a tab for this site".to_string())
}

fn run() -> Result<Value, String> {
    let args = parse_args()?;
    let url = normalize_url(&args.url);
    let site = Site::detect(&url).ok_or_else(|| {
        format!("unsupported site '{}': expected chatgpt / gemini / kimi / deepseek", args.url)
    })?;
    let (base, token) = load_config()?;
    let c = Client::new(base, token);
    let started = Instant::now();

    if args.image && !site.supports_image() {
        return Err(format!("--image is only supported on chatgpt (got {})", site.name()));
    }

    let app = site.app_url().to_string(); // always drive the real chat app
    let tab = acquire_tab(&c, site, &app, args.new_tab)?;
    c.focus(&tab);
    // Never block images in image mode — we need the generated image to load.
    c.post(&format!("/tabs/{tab}/navigate"), json!({ "url": app, "blockImages": !args.image }))?;

    let composer = wait_until(COMPOSER_TIMEOUT_S, 400, || c.js_true(&tab, site.composer_present_js()));
    if !composer {
        return Err(format!(
            "composer never appeared on {} — are you logged in? (open the site and sign in first)",
            site.name()
        ));
    }

    c.focus(&tab);

    // --list-models: report the site's model options and stop.
    if args.list_models {
        let models = list_models(&c, &tab, site)?;
        return Ok(json!({
            "model": site.name(),
            "url": app,
            "response": if models.is_empty() {
                format!("no switchable models on {} (gated on this tier/login)", site.name())
            } else {
                models.join("\n")
            },
        }));
    }

    // --model: switch before typing the prompt.
    if let Some(m) = &args.model {
        switch_model(&c, &tab, site, m)?;
    }

    // In image mode, prepend an explicit image-generation instruction.
    let eff_prompt = if args.image {
        format!("Create an image based on this description: {}", args.prompt)
    } else {
        args.prompt.clone()
    };
    let lit = serde_json::to_string(&eff_prompt).unwrap();
    let need = (eff_prompt.chars().filter(|c| !c.is_whitespace()).count() / 2).max(1);
    // Inject with retry: some editors (Kimi's Lexical) start contenteditable=false
    // and need a real click + a beat before insertText registers. Keep trying
    // until the text actually lands, so we never submit an empty composer.
    let mut got = 0usize;
    let mut last_err = String::from("injection failed");
    for _ in 0..8 {
        if site.needs_click_activation() {
            let _ = c.post(
                &format!("/tabs/{tab}/action"),
                json!({ "kind": "click", "selector": site.composer_sel() }),
            );
            std::thread::sleep(Duration::from_millis(300));
        }
        let inj = c.run_js(&tab, &site.inject_js(&lit))?;
        if inj.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            got = inj.get("len").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if got >= need {
                break;
            }
        } else if let Some(e) = inj.get("err").and_then(|v| v.as_str()) {
            last_err = e.to_string();
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    if got < need {
        return Err(format!(
            "prompt did not land in {} composer (got {} chars, expected ~{}): {}",
            site.name(), got, need, last_err
        ));
    }

    let baseline_len = as_text(&c.run_js(&tab, site.response_js())?).chars().count();

    send(&c, &tab, site)?;

    // Image mode: wait for the generated <img>, then download it.
    if args.image {
        let src = await_image(&c, &tab, site, &args)?;
        let out = resolve_out_path(args.out.as_deref());
        download_image(&c, &tab, &src, &out)?;
        return Ok(json!({
            "model": site.name(),
            "url": app,
            "prompt": args.prompt,
            "image_path": out,
            "elapsed_ms": started.elapsed().as_millis(),
        }));
    }

    let answer = await_response(&c, &tab, site, baseline_len, &args)?;

    Ok(json!({
        "model": site.name(),
        "url": app,
        "prompt": args.prompt,
        "response": answer,
        "elapsed_ms": started.elapsed().as_millis(),
    }))
}

fn send(c: &Client, tab: &str, site: Site) -> Result<(), String> {
    if let Some(sel) = site.send_button_sel() {
        let expr = format!(
            "(function(){{var b=document.querySelector('{sel}');\
             if(!b)return {{ok:false,err:'no send button'}};\
             if(b.disabled||b.getAttribute('aria-disabled')==='true')return {{ok:false,err:'disabled'}};\
             b.click();return {{ok:true}};}})()"
        );
        // Best-effort: clicking send can navigate the page (Claude /new -> /chat/<id>),
        // which makes the eval throw a detached/navigation error even though the click
        // registered. Swallow it — await_response is the real success signal.
        if let Ok(r) = c.run_js(tab, &expr) {
            if r.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                return Ok(());
            }
        } else {
            return Ok(());
        }
    }
    // Enter-dispatch submit (Kimi/DeepSeek default, and fallback if a click failed).
    let _ = c.run_js(tab, &site.enter_submit_js());
    Ok(())
}

/// Click whichever element is currently marked with data-gptbclick. Tries the
/// real compositor click first (needed for Angular/Gemini, which ignores synthetic
/// events), then falls back to a synthetic pointer sequence (needed for Claude's
/// fixed-header button, where the compositor click hits a scroll-into-view timeout,
/// and Kimi's div trigger). Then removes the marker.
fn click_marked(c: &Client, tab: &str) {
    let real_ok = c
        .post(&format!("/tabs/{tab}/action"), json!({ "kind": "click", "selector": "[data-gptbclick]" }))
        .ok()
        .and_then(|v| v.get("success").and_then(|s| s.as_bool()))
        .unwrap_or(false);
    if !real_ok {
        synth_click_marked(c, tab);
    }
    unmark(c, tab);
}

/// Synthetic pointer sequence on the marked element (does not unmark).
fn synth_click_marked(c: &Client, tab: &str) {
    let _ = c.run_js(tab, "(function(){var el=document.querySelector('[data-gptbclick]');if(el){['pointerdown','mousedown','pointerup','mouseup','click'].forEach(function(t){el.dispatchEvent(new MouseEvent(t,{bubbles:true,cancelable:true,view:window}));});}return 1;})()");
}

fn unmark(c: &Client, tab: &str) {
    let _ = c.run_js(tab, "(function(){var el=document.querySelector('[data-gptbclick]');if(el)el.removeAttribute('data-gptbclick');return 1;})()");
}

/// Count option elements currently visible for a site's picker.
fn options_count(c: &Client, tab: &str, site: Site) -> usize {
    let optsel = site.model_option_sel();
    let js = format!("(function(){{var n=0;'{optsel}'.split(',').forEach(function(s){{n+=document.querySelectorAll(s.trim()).length;}});return n;}})()");
    c.run_js(tab, &js).ok().and_then(|v| v.as_u64()).unwrap_or(0) as usize
}

/// Mark an element by CSS selector; returns whether it was found.
fn mark_selector(c: &Client, tab: &str, sel: &str) -> bool {
    let js = format!("(function(){{var el=document.querySelector('{sel}');if(!el)return false;el.setAttribute('data-gptbclick','1');return true;}})()");
    c.run_js(tab, &js).ok().and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Mark the first menuitem/button/div/span/a whose text contains `text`.
fn mark_text(c: &Client, tab: &str, text: &str) -> bool {
    let js = format!(
        "(function(){{var t='{text}'.toLowerCase();var ns=document.querySelectorAll('[role=menuitem],button,div,span,a');\
         for(var i=0;i<ns.length;i++){{if((ns[i].innerText||'').trim().toLowerCase().indexOf(t)>=0){{ns[i].setAttribute('data-gptbclick','1');return true;}}}}return false;}})()"
    );
    c.run_js(tab, &js).ok().and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Open a site's model picker (with its optional "more models" expander).
fn open_picker(c: &Client, tab: &str, site: Site) -> Result<(), String> {
    let open = site.model_picker_open_sel().ok_or_else(|| format!("{} has no model picker", site.name()))?;
    c.focus(tab);
    // The picker button can render a beat after the composer — wait for it.
    let present_js = format!("!!document.querySelector('{open}')");
    if !wait_until(6, 300, || c.js_true(tab, &present_js)) {
        return Err(format!("model picker not found on {}", site.name()));
    }
    mark_selector(c, tab, open);
    click_marked(c, tab);
    std::thread::sleep(Duration::from_millis(900));
    // Some triggers (Claude's fixed-header button) report a successful compositor
    // click without opening the menu — force a synthetic click if nothing appeared.
    if options_count(c, tab, site) == 0 {
        mark_selector(c, tab, open);
        synth_click_marked(c, tab);
        unmark(c, tab);
        std::thread::sleep(Duration::from_millis(900));
    }
    Ok(())
}

fn close_picker(c: &Client, tab: &str) {
    let _ = c.run_js(tab, "(function(){document.body.click();return 1;})()");
}

/// Poll the open picker for its option labels (first line of each option).
fn read_options(c: &Client, tab: &str, site: Site) -> Vec<String> {
    let optsel = site.model_option_sel();
    let js = format!(
        "(function(){{var out=[];'{optsel}'.split(',').forEach(function(s){{\
         document.querySelectorAll(s.trim()).forEach(function(e){{var t=(e.innerText||'').trim().split('\\n')[0];\
         if(t&&out.indexOf(t)<0)out.push(t);}});}});return out;}})()"
    );
    for _ in 0..8 {
        if let Ok(v) = c.run_js(tab, &js) {
            let arr: Vec<String> = v
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            if !arr.is_empty() {
                return arr;
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    vec![]
}

/// Mark the option whose first-line text matches `want`. Returns (found, available).
fn mark_option(c: &Client, tab: &str, site: Site, want: &str) -> (bool, Vec<String>) {
    let optsel = site.model_option_sel();
    let js = format!(
        "(function(){{var want='{want}';var opts=[];'{optsel}'.split(',').forEach(function(s){{\
         document.querySelectorAll(s.trim()).forEach(function(e){{opts.push(e);}});}});\
         var avail=[];for(var i=0;i<opts.length;i++){{var txt=(opts[i].innerText||'').trim().split('\\n')[0];\
         if(!txt)continue;if(avail.indexOf(txt)<0)avail.push(txt);\
         if(txt.toLowerCase().replace(/\\s+/g,'').indexOf(want)>=0){{opts[i].setAttribute('data-gptbclick','1');\
         return {{found:true}};}}}}return {{found:false,available:avail}};}})()"
    );
    match c.run_js(tab, &js) {
        Ok(r) => {
            let found = r.get("found").and_then(|v| v.as_bool()).unwrap_or(false);
            let avail = r
                .get("available")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            (found, avail)
        }
        Err(_) => (false, vec![]),
    }
}

/// Read the model options a site's picker currently offers.
fn list_models(c: &Client, tab: &str, site: Site) -> Result<Vec<String>, String> {
    if matches!(site, Site::DeepSeek) {
        return Ok(vec!["DeepThink".into(), "Search".into()]);
    }
    open_picker(c, tab, site)?;
    let mut models = read_options(c, tab, site);
    // Reveal extra models behind an expander (Claude's "More models"), if any.
    if let Some(exp) = site.model_expand_text() {
        if mark_text(c, tab, exp) {
            click_marked(c, tab);
            std::thread::sleep(Duration::from_millis(500));
            for m in read_options(c, tab, site) {
                if !models.contains(&m) {
                    models.push(m);
                }
            }
        }
    }
    close_picker(c, tab);
    Ok(models)
}

/// Switch the site's model before sending. Best-effort with a clear error listing
/// available models when the requested one isn't offered (e.g. gated on free tier).
fn switch_model(c: &Client, tab: &str, site: Site, model: &str) -> Result<(), String> {
    c.focus(tab);
    // DeepSeek: independent DeepThink / Search toggles, not a model picker.
    if matches!(site, Site::DeepSeek) {
        let want = model.to_lowercase();
        let target = if want.contains("search") { "Search" } else { "DeepThink" };
        let js = format!(
            "(function(){{var target='{target}';var ns=document.querySelectorAll('div.ds-toggle-button');\
             for(var i=0;i<ns.length;i++){{if((ns[i].innerText||'').trim().indexOf(target)>=0){{\
             var pressed=ns[i].getAttribute('aria-pressed')==='true';\
             if(!pressed)ns[i].setAttribute('data-gptbclick','1');\
             return {{found:true,pressed:pressed}};}}}}return {{found:false}};}})()"
        );
        let r = c.run_js(tab, &js)?;
        if r.get("found").and_then(|v| v.as_bool()).unwrap_or(false) {
            if !r.get("pressed").and_then(|v| v.as_bool()).unwrap_or(false) {
                click_marked(c, tab);
            }
            return Ok(());
        }
        return Err(format!("deepseek: could not find toggle for '{model}' (try 'deepthink' or 'search')"));
    }

    open_picker(c, tab, site)?;
    let _ = read_options(c, tab, site); // ensure options rendered before matching
    let want: String = model.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    let (mut found, mut avail) = mark_option(c, tab, site, &want);
    if !found {
        if let Some(exp) = site.model_expand_text() {
            if mark_text(c, tab, exp) {
                click_marked(c, tab);
                std::thread::sleep(Duration::from_millis(500));
                let (f2, a2) = mark_option(c, tab, site, &want);
                found = f2;
                if !a2.is_empty() {
                    avail = a2;
                }
            }
        }
    }
    if found {
        click_marked(c, tab);
        std::thread::sleep(Duration::from_millis(600));
        return Ok(());
    }
    close_picker(c, tab);
    if avail.is_empty() {
        Err(format!("model '{model}' not selectable on {} (picker empty — likely gated on this tier/login)", site.name()))
    } else {
        Err(format!("model '{model}' not found on {}. Available: {}", site.name(), avail.join(", ")))
    }
}

fn await_response(c: &Client, tab: &str, site: Site, baseline_len: usize, args: &Args) -> Result<String, String> {
    let poll = Duration::from_millis(args.poll_ms);
    let deadline = Instant::now() + Duration::from_secs(args.timeout_s);
    let first_token_deadline = Instant::now() + Duration::from_secs(FIRST_TOKEN_TIMEOUT_S);
    let probe = format!(
        "(function(){{var t={};return {{len:(t||'').length,gen:!!({})}};}})()",
        site.response_js(),
        site.generating_js()
    );

    let mut started_gen = false;
    while Instant::now() < first_token_deadline {
        c.focus(tab);
        let p = c.run_js(tab, &probe)?;
        let len = p.get("len").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let gen = p.get("gen").and_then(|v| v.as_bool()).unwrap_or(false);
        if gen || len > baseline_len {
            started_gen = true;
            break;
        }
        std::thread::sleep(poll);
    }
    if !started_gen {
        return Err(format!("{} produced no response within {}s of sending", site.name(), FIRST_TOKEN_TIMEOUT_S));
    }

    let mut last_len = 0usize;
    let mut stable_count = 0u64;
    loop {
        c.focus(tab);
        let p = c.run_js(tab, &probe)?;
        let len = p.get("len").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let gen = p.get("gen").and_then(|v| v.as_bool()).unwrap_or(false);

        if len == last_len && !gen && len > baseline_len {
            stable_count += 1;
        } else {
            stable_count = 0;
        }
        last_len = len;

        if stable_count >= args.stable {
            break;
        }
        if Instant::now() >= deadline {
            eprintln!("gptbrowser: warning — hit {}s timeout; returning partial response", args.timeout_s);
            break;
        }
        std::thread::sleep(poll);
    }

    let final_text = as_text(&c.run_js(tab, site.response_js())?);
    if final_text.trim().is_empty() {
        return Err(format!("{}: response came back empty", site.name()));
    }
    Ok(final_text)
}

fn as_text(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// Poll for a generated image after an image-mode prompt. Errors on the site's
/// "image generation failed" message. Image gen is slow (up to a few minutes).
fn await_image(c: &Client, tab: &str, site: Site, args: &Args) -> Result<String, String> {
    let poll = Duration::from_millis(args.poll_ms.max(1500));
    let deadline = Instant::now() + Duration::from_secs(args.timeout_s.max(240));
    loop {
        c.focus(tab);
        // hard failure from the site itself
        let failed = c
            .run_js(tab, "(function(){return /image generation failed/i.test((document.querySelector('main')||{}).innerText||'');})()")
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if failed {
            return Err(format!("{} reported 'image generation failed' (often a free-tier daily image cap — try later or on a paid plan)", site.name()));
        }
        let src = as_text(&c.run_js(tab, site.image_src_js())?);
        if !src.is_empty() {
            return Ok(src);
        }
        if Instant::now() >= deadline {
            return Err(format!("{}: no image appeared within {}s", site.name(), args.timeout_s.max(240)));
        }
        std::thread::sleep(poll);
    }
}

/// Default output path when --out isn't given.
fn resolve_out_path(out: Option<&str>) -> String {
    if let Some(o) = out {
        return o.to_string();
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("gptbrowser-image-{secs}.png")
}

/// Percent-encode everything except RFC-3986 unreserved chars (for /download?url=).
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Download the generated image to `out`. http(s) URLs go through PinchTab's
/// server-side /download (uses the browser session, bypasses CORS); blob: URLs
/// are read in-page and base64-decoded here.
fn download_image(c: &Client, tab: &str, src: &str, out: &str) -> Result<(), String> {
    if src.starts_with("blob:") {
        // Read the blob in-page and return a data URL (awaitPromise resolves it).
        let expr = format!(
            "fetch({}).then(function(r){{return r.blob();}}).then(function(b){{return new Promise(function(res){{var fr=new FileReader();fr.onloadend=function(){{res(fr.result);}};fr.readAsDataURL(b);}});}})",
            serde_json::to_string(src).unwrap()
        );
        let v = c.post(
            &format!("/tabs/{tab}/evaluate"),
            json!({ "expression": expr, "awaitPromise": true }),
        )?;
        let data_url = v.get("result").and_then(|x| x.as_str()).unwrap_or("");
        let b64 = data_url.splitn(2, ",").nth(1).ok_or("blob: could not read image data")?;
        let bytes = base64_decode(b64)?;
        std::fs::write(out, &bytes).map_err(|e| format!("write {out}: {e}"))?;
        Ok(())
    } else {
        // Fetch raw bytes through PinchTab (uses the browser session, bypasses CORS)
        // and write them ourselves — server-side output=file rejects absolute paths.
        let url = percent_encode(src);
        let bytes = c.get_bytes(&format!("/download?url={url}&raw=true"))?;
        if bytes.len() < 100 {
            return Err(format!("downloaded image was empty/too small ({} bytes)", bytes.len()));
        }
        std::fs::write(out, &bytes).map_err(|e| format!("write {out}: {e}"))?;
        Ok(())
    }
}

/// Minimal standard-base64 decoder (no deps).
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &c in s.as_bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' {
            continue;
        }
        let v = val(c).ok_or("invalid base64")? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

fn wait_until<F: Fn() -> bool>(timeout_s: u64, step_ms: u64, f: F) -> bool {
    let deadline = Instant::now() + Duration::from_secs(timeout_s);
    while Instant::now() < deadline {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(step_ms));
    }
    false
}

const HELP: &str = "\
gptbrowser — prompt a logged-in chatbot web UI via PinchTab, get clean text back.

USAGE:
  gptbrowser <url> <prompt...>
  gptbrowser --url <url> --prompt \"<text>\"
  echo \"<prompt>\" | gptbrowser <url>

SUPPORTED URLS:
  chatgpt.com   gemini.com   kimi.com   chat.deepseek.com
  claude.ai     perplexity.ai   chat.z.ai
  copilot.microsoft.com   grok.com   (require you to be signed in)

FLAGS:
  --model <name>    switch model before sending (fuzzy match; see --list-models)
  --list-models     print the site's selectable models and exit
  --image           generate an image (chatgpt only) and download it; prints the file path
  --out <path>      output file for --image (default gptbrowser-image-<epoch>.png)
  --json            emit structured JSON instead of bare text
  --new-tab         force a new tab instead of reusing an open one
  --timeout <sec>   overall generation budget (default 240)
  --poll <ms>       poll interval while generating (default 700)
  --stable <n>      no-growth polls that mark completion (default 6)

MODEL SWITCHING works on: gemini, kimi, claude, deepseek (DeepThink/Search toggle).
  chatgpt / perplexity / z.ai gate model choice behind a paid tier or login.

Assumes you are already logged in to the site in the PinchTab-controlled Chrome.";

fn main() {
    match run() {
        Ok(out) => {
            let json_mode = std::env::args().any(|a| a == "--json");
            if json_mode {
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else if let Some(p) = out.get("image_path").and_then(|v| v.as_str()) {
                println!("{p}");
            } else {
                println!("{}", out.get("response").and_then(|v| v.as_str()).unwrap_or(""));
            }
        }
        Err(e) if e == "HELP" => println!("{HELP}"),
        Err(e) => {
            eprintln!("gptbrowser: error: {e}");
            std::process::exit(1);
        }
    }
}
