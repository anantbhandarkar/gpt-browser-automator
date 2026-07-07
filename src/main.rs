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
        }
    }

    fn composer_present_js(&self) -> &'static str {
        match self {
            Site::ChatGpt => "!!document.querySelector('#prompt-textarea')",
            Site::Gemini => "!!document.querySelector('.ql-editor')",
            Site::Kimi => "!!document.querySelector('.chat-input-editor')",
            Site::DeepSeek => "!!document.querySelector('#chat-input, textarea')",
        }
    }

    /// JS IIFE returning {ok, len, err}. Injects `lit` (a JS string literal).
    /// Kimi's Lexical editor won't accept a Range-based insertText until a real
    /// pointer interaction initializes its internal selection — so click first.
    fn needs_click_activation(&self) -> bool {
        matches!(self, Site::Kimi)
    }

    /// Primary CSS selector for the composer element.
    fn composer_sel(&self) -> &'static str {
        match self {
            Site::ChatGpt => "#prompt-textarea",
            Site::Gemini => ".ql-editor",
            Site::Kimi => ".chat-input-editor",
            Site::DeepSeek => "#chat-input",
        }
    }

    /// JS IIFE returning {ok, len}. `len` is the whitespace-stripped length of
    /// the composer after injection — the caller checks it actually landed.
    fn inject_js(&self, lit: &str) -> String {
        match self {
            // ProseMirror (ChatGPT) / Quill (Gemini): selectAll+delete then insertText.
            Site::ChatGpt | Site::Gemini => {
                let sel = self.composer_sel();
                format!(
                    "(function(){{var el=document.querySelector('{sel}');\
                     if(!el)return {{ok:false,err:'composer not found'}};\
                     el.focus();\
                     try{{document.execCommand('selectAll',false,null);document.execCommand('delete',false,null);}}catch(e){{}}\
                     document.execCommand('insertText',false,{lit});\
                     return {{ok:true,len:(el.innerText||el.textContent||'').replace(/\\s+/g,'').length}};}})()"
                )
            }
            // Kimi (Lexical): execCommand('delete') wipes the selection and makes
            // insertText no-op. Instead select all contents via a Range, then
            // insertText — which REPLACES the selection. No execCommand delete.
            Site::Kimi => format!(
                "(function(){{var el=document.querySelector('.chat-input-editor');\
                 if(!el)return {{ok:false,err:'composer not found'}};el.focus();\
                 var r=document.createRange();r.selectNodeContents(el);\
                 var s=getSelection();s.removeAllRanges();s.addRange(r);\
                 document.execCommand('insertText',false,{lit});\
                 return {{ok:true,len:(el.innerText||el.textContent||'').replace(/\\s+/g,'').length}};}})()"
            ),
            // DeepSeek: real <textarea> — React-safe native value setter + input event.
            Site::DeepSeek => format!(
                "(function(){{var el=document.querySelector('#chat-input')||document.querySelector('textarea');\
                 if(!el)return {{ok:false,err:'composer not found'}};el.focus();\
                 var s=Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype,'value').set;\
                 s.call(el,{lit});el.dispatchEvent(new Event('input',{{bubbles:true}}));\
                 return {{ok:true,len:(el.value||'').replace(/\\s+/g,'').length}};}})()"
            ),
        }
    }

    /// Send button selector, or None for sites where we submit via Enter.
    fn send_button_sel(&self) -> Option<&'static str> {
        match self {
            Site::ChatGpt => Some("button[data-testid=\"send-button\"]"),
            Site::Gemini => Some("button[aria-label=\"Send message\"]"),
            // Kimi's .send-button-container is a div; Enter-dispatch is more reliable.
            Site::Kimi => None,
            Site::DeepSeek => None,
        }
    }

    /// JS IIFE that dispatches a synthetic Enter keydown on the composer to
    /// submit. Reliable for textarea + Lexical where a CDP key press isn't.
    fn enter_submit_js(&self) -> String {
        let sel = match self {
            Site::DeepSeek => "document.querySelector('#chat-input')||document.querySelector('textarea')".to_string(),
            _ => format!("document.querySelector('{}')", self.composer_sel()),
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
        }
    }

    /// JS boolean: currently generating? Used ONLY to *block* premature "done".
    fn generating_js(&self) -> &'static str {
        match self {
            Site::ChatGpt => "!!document.querySelector('button[data-testid=\"stop-button\"]')",
            _ => "false",
        }
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
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut url = String::new();
    let mut prompt_parts: Vec<String> = Vec::new();
    let mut new_tab = false;
    let mut poll_ms = DEFAULT_POLL_MS;
    let mut stable = DEFAULT_STABLE_POLLS;
    let mut timeout_s = DEFAULT_TOTAL_TIMEOUT_S;

    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        match a.as_str() {
            "--json" => {} // handled in main() via env scan
            "--new-tab" => new_tab = true,
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
    if prompt.is_empty() {
        return Err("no prompt given (pass as args or via stdin)".to_string());
    }
    Ok(Args { url, prompt, new_tab, poll_ms, stable, timeout_s })
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

    let app = site.app_url().to_string(); // always drive the real chat app
    let tab = acquire_tab(&c, site, &app, args.new_tab)?;
    c.focus(&tab);
    c.post(&format!("/tabs/{tab}/navigate"), json!({ "url": app, "blockImages": true }))?;

    let composer = wait_until(COMPOSER_TIMEOUT_S, 400, || c.js_true(&tab, site.composer_present_js()));
    if !composer {
        return Err(format!(
            "composer never appeared on {} — are you logged in? (open the site and sign in first)",
            site.name()
        ));
    }

    c.focus(&tab);
    let lit = serde_json::to_string(&args.prompt).unwrap();
    let need = (args.prompt.chars().filter(|c| !c.is_whitespace()).count() / 2).max(1);
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
        let r = c.run_js(tab, &expr)?;
        if r.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Ok(());
        }
    }
    // Enter-dispatch submit (Kimi/DeepSeek default, and fallback if a click failed).
    c.run_js(tab, &site.enter_submit_js())?;
    Ok(())
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

FLAGS:
  --json            emit structured JSON instead of bare text
  --new-tab         force a new tab instead of reusing an open one
  --timeout <sec>   overall generation budget (default 240)
  --poll <ms>       poll interval while generating (default 700)
  --stable <n>      no-growth polls that mark completion (default 6)

Assumes you are already logged in to the site in the PinchTab-controlled Chrome.";

fn main() {
    match run() {
        Ok(out) => {
            let json_mode = std::env::args().any(|a| a == "--json");
            if json_mode {
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
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
