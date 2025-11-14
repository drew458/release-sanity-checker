package main

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
	"strings"

	_ "github.com/mattn/go-sqlite3"
)

// initDB creates the database file and schema if they don't exist.
func initDB(dataSourceName string) (*sql.DB, error) {
	db, err := sql.Open("sqlite3", dataSourceName)
	if err != nil {
		return nil, err
	}

	// Set WAL mode for better concurrency
	if _, err := db.Exec("PRAGMA journal_mode=WAL;"); err != nil {
		slog.Info("Failed to set WAL mode", "error", err)
	}
	if _, err := db.Exec("PRAGMA synchronous=NORMAL;"); err != nil {
		slog.Info("Failed to set synchronous=NORMAL", "error", err)
	}

	query := `
	CREATE TABLE IF NOT EXISTS response (
		request_id              TEXT NOT NULL,
		url                     TEXT NOT NULL, 
		baseline_status_code    INTEGER,
		checktime_status_code   INTEGER,
		baseline_headers        TEXT,
		checktime_headers       TEXT,
		baseline_body           TEXT,
		checktime_body          TEXT,
		PRIMARY KEY(request_id)
	);
	CREATE INDEX IF NOT EXISTS url_idx ON response(request_id);
	`
	if _, err := db.Exec(query); err != nil {
		return nil, fmt.Errorf("failed to create table: %w", err)
	}

	return db, nil
}

// findPreviousResponse queries for the stored baseline response.
func findPreviousResponse(db *sql.DB, requestID string, headersIgnored bool) (*HttpResponseData, error) {
	var query string
	if headersIgnored {
		query = "SELECT baseline_status_code, baseline_body FROM response WHERE request_id = ?"
	} else {
		query = "SELECT baseline_status_code, baseline_body, baseline_headers FROM response WHERE request_id = ?"
	}

	var statusCode int
	var body, headersJSON string
	var headers http.Header

	var row *sql.Row
	if headersIgnored {
		row = db.QueryRow(query, requestID)
		err := row.Scan(&statusCode, &body)
		if err == sql.ErrNoRows {
			return nil, nil // No baseline found, not an error
		}
		if err != nil {
			return nil, err
		}
		headers = make(http.Header)
	} else {
		row = db.QueryRow(query, requestID)
		err := row.Scan(&statusCode, &body, &headersJSON)
		if err == sql.ErrNoRows {
			return nil, nil // No baseline found
		}
		if err != nil {
			return nil, err
		}
		// Unmarshal headers from http.Header format
		if err := json.Unmarshal([]byte(headersJSON), &headers); err != nil {
			slog.Info("Warning: could not parse baseline headers for %s: %v", requestID, err)
			headers = make(http.Header)
		}
	}

	// Reconstruct the response
	parsedBody := ParsedBody{
		Raw:  body,
		JSON: nil,
	}
	if strings.HasPrefix(headers.Get("Content-Type"), "application/json") {
		var jsonData any
		if err := json.Unmarshal([]byte(body), &jsonData); err == nil {
			parsedBody.JSON = jsonData
		}
	}

	return &HttpResponseData{
		StatusCode: statusCode,
		Headers:    headers,
		Body:       parsedBody,
	}, nil
}

// saveResponse inserts or updates the response in the database.
func saveResponse(
	db *sql.DB,
	requestID string,
	flow *RequestConfig,
	response *HttpResponseData,
	isBaseline bool,
) error {
	var queryStr string
	headersJSON, err := json.Marshal(response.Headers)
	if err != nil {
		return fmt.Errorf("failed to marshal headers: %w", err)
	}

	if isBaseline {
		queryStr = `
		INSERT INTO response (request_id, url, baseline_status_code, baseline_body, baseline_headers)
		VALUES (?, ?, ?, ?, ?)
		ON CONFLICT (request_id) DO UPDATE SET 
			url = excluded.url, 
			baseline_status_code = excluded.baseline_status_code,
			baseline_body = excluded.baseline_body,
			baseline_headers = excluded.baseline_headers
		`
	} else {
		queryStr = `
		INSERT INTO response (request_id, url, checktime_status_code, checktime_body, checktime_headers)
		VALUES (?, ?, ?, ?, ?)
		ON CONFLICT (request_id) DO UPDATE SET 
			url = excluded.url,
			checktime_status_code = excluded.checktime_status_code,
			checktime_body = excluded.checktime_body,
			checktime_headers = excluded.checktime_headers
		`
	}

	_, err = db.Exec(
		queryStr,
		requestID,
		flow.URL,
		response.StatusCode,
		response.Body.Raw,
		string(headersJSON),
	)
	return err
}
