package main

import (
	"encoding/json"
	"net/http"
)

// ParsedBody holds the raw string body and a parsed JSON representation if possible.
type ParsedBody struct {
	Raw  string // raw response body as string
	JSON any    // unmarshaled JSON (interface{})
}

// HttpResponseData holds all relevant info from an HTTP response.
type HttpResponseData struct {
	StatusCode int
	Headers    http.Header
	Body       ParsedBody
}

// RequestConfig defines a single HTTP request within a flow.
type RequestConfig struct {
	URL     string            `json:"url"`
	Headers map[string]string `json:"headers"`
	// Use json.RawMessage to delay parsing of the body
	Body json.RawMessage `json:"body"`
}

// RequestFlowConfig defines a full request flow.
type RequestFlowConfig struct {
	ID          string          `json:"id"`
	Flow        []RequestConfig `json:"flow"`
	IgnorePaths []string        `json:"ignore_paths"`
}

// SanityCheckConfig is the root object for a config file.
type SanityCheckConfig struct {
	Requests []RequestFlowConfig `json:"requests"`
}
