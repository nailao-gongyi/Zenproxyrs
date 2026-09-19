#!/usr/bin/env python3
"""把 grok-free-register 的 ss:// / vless:// 节点列表转成 sing-box 配置:
每节点一个 SOCKS5 inbound(127.0.0.1:20001+) + 对应 outbound tag。"""
import base64, json, re, sys
from urllib.parse import urlparse, parse_qs, unquote

LIST_FILE = sys.argv[1] if len(sys.argv) > 1 else "/root/grok-free-register/代理-relay.txt.disabled"
OUT = sys.argv[2] if len(sys.argv) > 2 else "/tmp/zen_relay_config.json"
PORT_BASE = 20001


def b64d(s):
    s = s.strip()
    s += "=" * (-len(s) % 4)
    return base64.b64decode(s).decode("utf-8", "replace")


def parse_ss(url):
    # ss://b64(method:password)@host:port#tag  或 ss://b64(method:password@host:port)#tag
    body = url[5:].split("#")[0]
    tag = url.split("#")[1] if "#" in url else "ss"
    if "@" in body:
        userinfo, hostport = body.rsplit("@", 1)
        userinfo = unquote(userinfo)
        try:
            userinfo = b64d(userinfo)
        except Exception:
            pass
        method, password = userinfo.split(":", 1)
        host, port = hostport.split(":")
    else:
        raw = b64d(body)
        userinfo, hostport = raw.rsplit("@", 1)
        method, password = userinfo.split(":", 1)
        host, port = hostport.split(":")
    return {
        "type": "shadowsocks", "tag": f"in-{re.sub(r'[^a-zA-Z0-9]', '', tag)}",
        "server": host, "server_port": int(port),
        "method": method, "password": password,
    }, tag


def parse_vless(url):
    u = urlparse(url)
    tag = unquote(u.fragment) if u.fragment else "vless"
    q = parse_qs(u.query)
    out = {
        "type": "vless", "tag": f"in-{re.sub(r'[^a-zA-Z0-9]', '', tag)}",
        "server": u.hostname, "server_port": u.port or 443,
        "uuid": u.username, "flow": q.get("flow", [""])[0],
    }
    security = q.get("security", ["none"])[0]
    if security == "tls":
        out["tls"] = {"enabled": True, "server_name": q.get("sni", [u.hostname])[0]}
    return out, tag


nodes, inbounds, tag_used = [], [], set()
for line in open(LIST_FILE, encoding="utf-8"):
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    try:
        if line.startswith("ss://"):
            node, tag = parse_ss(line)
        elif line.startswith("vless://"):
            node, tag = parse_vless(line)
        else:
            continue
        suffix = 2
        base_tag = node["tag"]
        while node["tag"] in tag_used:
            node["tag"] = f"{base_tag}{suffix}"
            suffix += 1
        tag_used.add(node["tag"])
        nodes.append(node)
    except Exception as e:
        print(f"skip: {line[:50]} -> {e}", file=sys.stderr)

for i, n in enumerate(nodes):
    port = PORT_BASE + i
    inbounds.append({
        "type": "socks", "tag": f"socks-{port}", "listen": "0.0.0.0", "listen_port": port,
    })
    n["detour"] = f"socks-{port}"  # sing-box: outbound 不用 detour 到 inbound; 见下

# sing-box 的正确关联方式: 每个 socks inbound 用 users 区分不可行; 正确做法是
# 每节点一个 inbound + 一个 outbound, 用 route 规则按 inbound tag 分流。
routes = [
    {"inbound": f"socks-{PORT_BASE + i}", "outbound": n["tag"]}
    for i, n in enumerate(nodes)
]

config = {
    "log": {"level": "warn"},
    "inbounds": inbounds,
    "outbounds": nodes + [{"type": "direct", "tag": "direct"}],
    "route": {"rules": routes, "final": "direct"},
}
json.dump(config, open(OUT, "w"), ensure_ascii=False, indent=1)
print(f"{len(nodes)} nodes -> {OUT}, socks ports {PORT_BASE}..{PORT_BASE + len(nodes) - 1}")
