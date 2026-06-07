const statusEl = document.querySelector("#status");
const sessionLine = document.querySelector("#sessionLine");
const messages = document.querySelector("#messages");
const eventLog = document.querySelector("#eventLog");
const promptForm = document.querySelector("#promptForm");
const promptInput = document.querySelector("#prompt");
const rawForm = document.querySelector("#rawForm");
const rawJson = document.querySelector("#rawJson");
const clearEvents = document.querySelector("#clearEvents");

let ws;
let nextId = 1;
let threadId = null;
let pendingPrompt = null;
let activeAgentMessage = null;
const pending = new Map();

boot();

async function boot() {
  const session = await fetch("/api/session").then((res) => res.json());
  sessionLine.textContent = session.sandbox
    ? `Session ${session.session_id.slice(0, 10)} · ${session.sandbox.name}`
    : `Session ${session.session_id.slice(0, 10)} · sandbox starts on first run`;
  connect();
}

function connect() {
  setStatus("busy", "connecting");
  const protocol = window.location.protocol === "https:" ? "wss" : "ws";
  ws = new WebSocket(`${protocol}://${window.location.host}/ws/codex`);

  ws.addEventListener("open", () => {
    setStatus("online", "online");
    send({
      method: "initialize",
      id: 0,
      params: {
        clientInfo: {
          name: "nju_cli_web_service",
          title: "NJU CLI Web Agent",
          version: "0.1.0",
        },
        capabilities: {
          experimentalApi: true,
        },
      },
    });
    send({ method: "initialized", params: {} });
    rawJson.value = JSON.stringify(
      {
        method: "thread/start",
        id: nextId,
        params: {},
      },
      null,
      2,
    );
  });

  ws.addEventListener("message", (event) => {
    const parsed = parseJson(event.data);
    appendEvent(parsed ?? event.data, "in");
    if (parsed) handleProtocolMessage(parsed);
  });

  ws.addEventListener("close", () => {
    setStatus("", "offline");
    appendMessage("meta", "Connection closed.");
  });

  ws.addEventListener("error", () => {
    setStatus("", "error");
    appendMessage("meta", "WebSocket error.");
  });
}

promptForm.addEventListener("submit", (event) => {
  event.preventDefault();
  const prompt = promptInput.value.trim();
  if (!prompt) return;
  promptInput.value = "";
  appendMessage("user", prompt);
  activeAgentMessage = null;

  if (!threadId) {
    pendingPrompt = prompt;
    const id = request("thread/start", {});
    pending.set(id, { kind: "thread-start" });
  } else {
    startTurn(prompt);
  }
});

rawForm.addEventListener("submit", (event) => {
  event.preventDefault();
  const payload = parseJson(rawJson.value);
  if (!payload) {
    appendMessage("meta", "Raw JSON is invalid.");
    return;
  }
  send(payload);
});

clearEvents.addEventListener("click", () => {
  eventLog.textContent = "";
});

function startTurn(prompt) {
  setStatus("busy", "running");
  request("turn/start", {
    threadId,
    input: [{ type: "text", text: prompt }],
  });
}

function request(method, params) {
  const id = nextId++;
  send({ method, id, params });
  return id;
}

function send(message) {
  appendEvent(message, "out");
  ws.send(JSON.stringify(message));
}

function handleProtocolMessage(message) {
  if (message.id !== undefined && pending.has(message.id)) {
    const entry = pending.get(message.id);
    pending.delete(message.id);
    if (entry.kind === "thread-start" && message.result?.thread?.id) {
      threadId = message.result.thread.id;
      sessionLine.textContent = `${sessionLine.textContent} · thread ${threadId}`;
      if (pendingPrompt) {
        const prompt = pendingPrompt;
        pendingPrompt = null;
        startTurn(prompt);
      }
    }
  }

  if (message.error) {
    setStatus("", "error");
    appendMessage("meta", `Error ${message.error.code}: ${message.error.message}`);
    return;
  }

  switch (message.method) {
    case "item/agentMessage/delta":
      appendAgentDelta(message.params?.delta ?? message.params?.text ?? "");
      break;
    case "turn/started":
      setStatus("busy", "running");
      appendMessage("meta", "Turn started.");
      break;
    case "turn/completed":
      setStatus("online", "online");
      activeAgentMessage = null;
      appendMessage("meta", `Turn completed: ${message.params?.status ?? "done"}.`);
      break;
    case "item/started":
      appendMessage("meta", summarizeItem("Started", message.params?.item));
      break;
    case "item/completed":
      appendMessage("meta", summarizeItem("Completed", message.params?.item));
      break;
    default:
      if (message.method && /approval|permission|confirm/i.test(message.method)) {
        appendMessage("meta", `${message.method}: use raw JSON panel to respond.`);
      }
  }
}

function appendAgentDelta(text) {
  if (!text) return;
  if (!activeAgentMessage) {
    activeAgentMessage = appendMessage("agent", "");
  }
  activeAgentMessage.textContent += text;
  messages.scrollTop = messages.scrollHeight;
}

function appendMessage(kind, text) {
  const node = document.createElement("div");
  node.className = `message ${kind}`;
  node.textContent = text;
  messages.append(node);
  messages.scrollTop = messages.scrollHeight;
  return node;
}

function appendEvent(value, direction) {
  const prefix = direction === "out" ? "client" : "server";
  const text = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  eventLog.textContent += `\n[${prefix}] ${text}\n`;
  eventLog.scrollTop = eventLog.scrollHeight;
}

function summarizeItem(prefix, item) {
  if (!item) return `${prefix} item.`;
  const kind = item.type ?? item.kind ?? "item";
  const title = item.title ?? item.command ?? item.name ?? item.id ?? "";
  return `${prefix} ${kind}${title ? `: ${title}` : ""}.`;
}

function setStatus(className, text) {
  statusEl.className = `status ${className}`;
  statusEl.textContent = text;
}

function parseJson(value) {
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

