package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// fetchWithRetries attempts to fetch a response, retrying on 5xx errors or network errors.
func fetchWithRetries(
	client *http.Client,
	reqConfig *RequestConfig,
	maxRetries int,
) (*HttpResponseData, error) {
	var lastErr error

	slog.Info("Sending request...", "url", reqConfig.URL)

	var method string
	var bodyReader io.Reader

	// If the request in the config has no body, it's a GET, otherwise a POST
	if reqConfig.Body == nil || string(reqConfig.Body) == "null" {
		method = http.MethodGet
		bodyReader = nil
	} else {
		method = http.MethodPost
		bodyReader = bytes.NewReader(reqConfig.Body)
	}

	// Create request
	req, err := http.NewRequest(method, reqConfig.URL, bodyReader)
	if err != nil {
		return nil, fmt.Errorf("failed to create request: %w", err)
	}

	// Set headers
	for k, v := range reqConfig.Headers {
		req.Header.Set(k, v)
	}

	for i := range maxRetries {
		// Send request
		resp, err := client.Do(req)

		// Retry if request failed
		if err != nil {
			slog.Info("Request failed", "Request URL", reqConfig.URL, "attempt", strconv.Itoa(i+1/maxRetries), "error", err.Error())
			lastErr = err
			time.Sleep(50 * time.Millisecond) // Backoff
			continue
		}
		defer resp.Body.Close()

		// Read response body
		bodyBytes, err := io.ReadAll(resp.Body)
		if err != nil {
			return nil, fmt.Errorf("failed to read response body: %w", err)
		}

		// Response is stored as a raw string by default
		bodyStr := string(bodyBytes)
		parsedBody := ParsedBody{
			Raw:  bodyStr,
			JSON: nil,
		}

		// Try to parse as JSON only if content-type indicates it
		contentType := resp.Header.Get("Content-Type")
		if strings.HasPrefix(contentType, "application/json") {
			var jsonData any
			if err := json.Unmarshal(bodyBytes, &jsonData); err == nil {
				parsedBody.JSON = jsonData
			}
		}

		respObj := &HttpResponseData{
			StatusCode: resp.StatusCode,
			Headers:    resp.Header,
			Body:       parsedBody,
		}

		// Retry on 5xx server errors
		if resp.StatusCode >= 500 {
			slog.Info("Request failed, retrying...", "url", reqConfig.URL, "status", strconv.Itoa(resp.StatusCode), "attempt", strconv.Itoa(i+1/maxRetries))
			lastErr = fmt.Errorf("server error: status code %d", resp.StatusCode)
			time.Sleep(50 * time.Millisecond) // Backoff
			continue
		}

		// Success (non-5xx, no error)
		slog.Info("Request done", "url", reqConfig.URL, "status", resp.StatusCode)
		return respObj, nil
	}

	return nil, fmt.Errorf("failed to get response for %s after %d retries: %w", reqConfig.URL, maxRetries, lastErr)
}
