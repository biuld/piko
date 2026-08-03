#!/bin/sh
# Minimal MCP server fixture for hostd tests (F-13): answers JSON-RPC
# requests over stdio with canned responses. Request ids are ignored — the
# provider reads one line per request in order.
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"fixture","version":"1.0"}}}'
      ;;
    *'"method":"notifications/initialized"'*)
      ;;
    *'"method":"tools/list"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"echo_tool","description":"Echo the input","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}]}}'
      ;;
    *'"method":"resources/list"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"resources":[{"uri":"file:///tmp/notes.md","name":"Notes","description":"Session notes","mimeType":"text/markdown"}]}}'
      ;;
    *'"method":"resources/templates/list"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":4,"result":{"resourceTemplates":[{"uriTemplate":"file:///tmp/{name}","name":"Temp files","mimeType":"text/plain"}]}}'
      ;;
    *'"method":"resources/read"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":5,"result":{"contents":[{"uri":"file:///tmp/notes.md","mimeType":"text/markdown","text":"hello from fixture"}]}}'
      ;;
    *'"method":"tools/call"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":6,"result":{"content":[{"type":"text","text":"echoed"}]}}'
      ;;
    *)
      printf '%s\n' '{"jsonrpc":"2.0","id":0,"error":{"code":-32601,"message":"method not found"}}'
      ;;
  esac
done
