#!/bin/sh
# MCP server fixture without resource support (F-13): tools work,
# resources/list and resources/templates/list fail closed.
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"fixture","version":"1.0"}}}'
      ;;
    *'"method":"notifications/initialized"'*)
      ;;
    *'"method":"tools/list"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"echo_tool","description":"Echo","inputSchema":{"type":"object"}}]}}'
      ;;
    *'"method":"resources/'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":3,"error":{"code":-32601,"message":"resources not supported"}}'
      ;;
    *)
      printf '%s\n' '{"jsonrpc":"2.0","id":0,"error":{"code":-32601,"message":"method not found"}}'
      ;;
  esac
done
