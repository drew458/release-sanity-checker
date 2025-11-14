package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"sync/atomic"
	"time"
)

// CliFlags holds the parsed command-line flags
type CliFlags struct {
	Directory     string
	IgnoreHeaders bool
	Baseline      bool
	Verbose       bool
}

func main() {
	// Parse command line flags
	var flags CliFlags
	flag.StringVar(&flags.Directory, "directory", "", "Run with all config files found in the directory.")
	flag.BoolVar(&flags.IgnoreHeaders, "ignore-headers", false, "Do not look for changes in response headers.")
	flag.BoolVar(&flags.Baseline, "baseline", false, "Build the baseline for the requests.")
	flag.BoolVar(&flags.Verbose, "verbose", false, "Print response that didn't change.")
	flag.Parse()

	// Parse Environment Variables
	requestsPerHost := 30
	if env := os.Getenv("REQUESTS_PER_HOST"); env != "" {
		if num, err := strconv.Atoi(env); err != nil {
			requestsPerHost = num
		}
	}
	maxRetries := 3
	if env := os.Getenv("MAX_RETRIES"); env != "" {
		if num, err := strconv.Atoi(env); err != nil {
			maxRetries = max(maxRetries, num)
		}
	}

	// Find and load config files
	configPaths, err := findConfigFiles(&flags)
	if err != nil {
		slog.Error(err.Error())
		return
	}
	if len(configPaths) == 0 {
		slog.Error("Error: No config file or directory specified.")
		return
	}

	// Initialize Database
	db, err := initDB("release-sanity-checker-data.db")
	if err != nil {
		slog.Error("Failed to initialize database", "error", err.Error())
		return
	}
	defer db.Close()

	httpClient := &http.Client{
		Transport: &http.Transport{
			MaxConnsPerHost: requestsPerHost,
		},
		Timeout: 10 * time.Second,
	}

	var wg sync.WaitGroup

	var requestsCounter atomic.Int64
	var changedCounter atomic.Int64
	var errorsCounter atomic.Int64

	// This channel will receive messages for printing differences
	printChan := make(chan PrintMessage, 100)
	// This channel signals that the printer is done
	doneChan := make(chan struct{})
	go runDifferencesPrinter(printChan, doneChan)

	fmt.Println("Starting to process requests...")

	// Process Config Files
	for _, configPath := range configPaths {
		slog.Info("Reading config", "path", configPath)
		config, err := loadConfig(configPath)
		if err != nil {
			slog.Info("Error reading config %s: %v. Skipping.", configPath, err)
			continue
		}

		// Process requests inside config file concurrently
		for _, reqConfig := range config.Requests {
			wg.Add(1)
			go func(rc RequestFlowConfig) {
				defer wg.Done()
				requestsCounter.Add(1)

				slog.Info("Checking request", "id", rc.ID)
				var currentResponse *HttpResponseData
				var currentFlowStep *RequestConfig
				var flowErr error

				// Flow is processed serially
				for i, flowStep := range rc.Flow {
					currentFlowStep = &rc.Flow[i] // Capture the pointer to the current step

					// Fetch response with retries
					resp, err := fetchWithRetries(httpClient, currentFlowStep, maxRetries)
					if err != nil {
						slog.Error("Failed to get response after multiple retries", "id", rc.ID, "url", flowStep.URL, "error", err.Error())
						flowErr = err // Store error and break flow
						break
					}
					currentResponse = resp // Store the response
				}

				// If the flow failed, increment error and stop
				if flowErr != nil {
					errorsCounter.Add(1)
					return
				}

				// Only the last request of the flow is checked
				if currentResponse == nil || currentFlowStep == nil {
					slog.Info("Request flow completed without any responses to check.", "id", rc.ID)
					return
				}

				// Compare or Baseline
				if !flags.Baseline {
					// Check mode: Find previous response and compare
					prevResponse, err := findPreviousResponse(db, rc.ID, flags.IgnoreHeaders)
					if err != nil {
						slog.Info("Error finding previous response for", "id", rc.ID, "error", err)
						errorsCounter.Add(1)
						return
					}

					if prevResponse != nil {
						// Convert ignore_paths to a map for O(1) lookup
						ignorePathsMap := make(map[string]struct{}, len(rc.IgnorePaths))
						for _, path := range rc.IgnorePaths {
							ignorePathsMap[path] = struct{}{}
						}

						differences := computeDifferences(prevResponse, currentResponse, flags.IgnoreHeaders, ignorePathsMap)

						if len(differences) == 0 {
							if flags.Verbose {
								fmt.Printf("\n✅ Request with ID: '%s' has not changed. ✅\n", rc.ID)
							}
						} else {
							changedCounter.Add(1)
							printChan <- PrintMessage{
								RequestID:   rc.ID,
								Differences: differences,
							}
						}
					}
				}

				// Save the response (either as baseline or checktime)
				err = saveResponse(db, rc.ID, currentFlowStep, currentResponse, flags.Baseline)
				if err != nil {
					slog.Info("Error saving response for '%s': %v", rc.ID, err)
					errorsCounter.Add(1)
				}
			}(reqConfig)
		}
	}

	// Wait for all processing to finish
	wg.Wait()

	// Shutdown Printer and wait for it to finish
	close(printChan) // Signal printer there are no more messages
	<-doneChan       // Wait for printer to signal it's done

	// Print Final Summary
	if flags.Baseline {
		fmt.Printf(
			"\nBaseline built successfully. Processed %d requests, errors: %d\n",
			requestsCounter.Load(),
			errorsCounter.Load(),
		)
	} else {
		fmt.Printf(
			"\nResponse check completed. Changed request: %d out of %d. Errors: %d\n",
			changedCounter.Load(),
			requestsCounter.Load(),
			errorsCounter.Load(),
		)
	}
}

func findConfigFiles(flags *CliFlags) ([]string, error) {
	var configPaths []string

	if flags.Directory != "" {
		// Read all .json files from the directory
		files, err := os.ReadDir(flags.Directory)
		if err != nil {
			return nil, fmt.Errorf("error: '%s' is not a valid directory: %w", flags.Directory, err)
		}
		foundFiles := false
		for _, file := range files {
			if !file.IsDir() && filepath.Ext(file.Name()) == ".json" {
				configPaths = append(configPaths, filepath.Join(flags.Directory, file.Name()))
				foundFiles = true
			}
		}
		if !foundFiles {
			slog.Warn("Warning: No JSON config files found", "directory", flags.Directory)
		}
	} else {
		// Use files provided as arguments
		configPaths = flag.Args()
	}
	return configPaths, nil
}

func loadConfig(path string) (*SanityCheckConfig, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var config SanityCheckConfig
	if err := json.Unmarshal(data, &config); err != nil {
		return nil, err
	}
	return &config, nil
}
