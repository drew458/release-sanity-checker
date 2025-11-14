package main

import (
	"fmt"
	"net/http"
	"reflect"
	"strings"
)

// DiffType defines the kind of difference found.
type DiffType int

const (
	StatusCodeChanged DiffType = iota
	HeaderValueChanged
	HeaderValueRemoved
	HeaderValueAdded
	BodyValueChanged
	BodyValueRemoved
	BodyValueAdded
	ArrayLengthChanged
	ArrayElementRemoved
	ArrayElementAdded
	DifferentBodyString
)

// Difference holds information about a single detected change.
type Difference struct {
	Type           DiffType
	Path           string
	OldVal         string
	NewVal         string
	HeaderName     string
	OldLen, NewLen int
}

const maxDepth = 10 // Recursion limit for JSON diff

// computeDifferences is the main entry point for diffing two responses.
func computeDifferences(
	resp1, resp2 *HttpResponseData,
	headersIgnored bool,
	ignoredPaths map[string]struct{},
) []Difference {
	var diffs []Difference

	// Compare Status Code
	if resp1.StatusCode != resp2.StatusCode {
		diffs = append(diffs, Difference{
			Type:   StatusCodeChanged,
			OldVal: fmt.Sprintf("%d", resp1.StatusCode),
			NewVal: fmt.Sprintf("%d", resp2.StatusCode),
		})
	}

	// Compare Headers
	if !headersIgnored {
		diffs = append(diffs, compareHeaders(resp1.Headers, resp2.Headers)...)
	}

	// Compare Bodies
	switch {
	case resp1.Body.JSON != nil && resp2.Body.JSON != nil:
		// Both are valid JSON, do a deep diff
		diffs = append(diffs, findJSONDifferences("", resp1.Body.JSON, resp2.Body.JSON, ignoredPaths, 0)...)
	default:
		// One or both are not JSON, do a raw string compare
		if resp1.Body.Raw != resp2.Body.Raw {
			diffs = append(diffs, Difference{
				Type:   DifferentBodyString,
				OldVal: resp1.Body.Raw,
				NewVal: resp2.Body.Raw,
			})
		}
	}

	return diffs
}

// compareHeaders diffs two http.Header maps.
func compareHeaders(h1, h2 http.Header) []Difference {
	var diffs []Difference
	keys1 := getHeaderKeys(h1)
	keys2 := getHeaderKeys(h2)

	// Find removed and changed headers
	for key := range keys1 {
		val1 := h1.Get(key)
		val2 := h2.Get(key)
		if _, found := keys2[key]; !found {
			diffs = append(diffs, Difference{
				Type:       HeaderValueRemoved,
				HeaderName: key,
			})
		} else if val1 != val2 {
			diffs = append(diffs, Difference{
				Type:       HeaderValueChanged,
				HeaderName: key,
				OldVal:     val1,
				NewVal:     val2,
			})
		}
	}

	// Find added headers
	for key := range keys2 {
		if _, ok := keys1[key]; !ok {
			diffs = append(diffs, Difference{
				Type:       HeaderValueAdded,
				HeaderName: key,
			})
		}
	}
	return diffs
}

func getHeaderKeys(h http.Header) map[string]struct{} {
	keys := make(map[string]struct{})
	for k := range h {
		keys[k] = struct{}{}
	}
	return keys
}

// findJSONDifferences recursively diffs two JSON structures (represented as `any`).
func findJSONDifferences(
	path string,
	v1, v2 any,
	ignoredPaths map[string]struct{},
	currentDepth int,
) []Difference {
	var diffs []Difference

	if currentDepth > maxDepth {
		return diffs
	}

	currentPath := "/" + path
	// Check if path should be ignored: complete match
	if _, ok := ignoredPaths[currentPath]; ok {
		return diffs
	}
	for ignoredPath := range ignoredPaths {
		if ignoredPath == "/" {
			continue
		}
		ignoredPath = strings.TrimSuffix(ignoredPath, "/")
		// Check if path should be ignored: the current path starts with something to ignore
		if currentPath == ignoredPath || strings.HasPrefix(currentPath, ignoredPath+"/") {
			return diffs
		}
	}

	// Use reflection to compare types
	map1, ok1 := v1.(map[string]any)
	map2, ok2 := v2.(map[string]any)
	arr1, ok3 := v1.([]any)
	arr2, ok4 := v2.([]any)

	switch {
	case ok1 && ok2: // Both are objects
		diffs = append(diffs, compareObjects(path, map1, map2, ignoredPaths, currentDepth)...)
	case ok3 && ok4: // Both are arrays
		diffs = append(diffs, compareArrays(path, arr1, arr2, ignoredPaths, currentDepth)...)
	default: // Primitives or type mismatch
		if !reflect.DeepEqual(v1, v2) {
			diffs = append(diffs, Difference{
				Type:   BodyValueChanged,
				Path:   path,
				OldVal: formatValue(v1, 50),
				NewVal: formatValue(v2, 50),
			})
		}
	}
	return diffs
}

// compareObjects diffs two JSON objects (maps).
func compareObjects(
	path string,
	map1, map2 map[string]any,
	ignoredPaths map[string]struct{},
	currentDepth int,
) []Difference {
	var diffs []Difference

	// Find removed and changed keys
	for key1, val1 := range map1 {
		newPath := buildPath(path, key1)
		val2, ok := map2[key1]
		if !ok {
			diffs = append(diffs, Difference{
				Type:   BodyValueRemoved,
				Path:   newPath,
				OldVal: formatValue(val1, 50),
			})
		} else {
			// Recurse
			diffs = append(diffs, findJSONDifferences(newPath, val1, val2, ignoredPaths, currentDepth+1)...)
		}
	}

	// Find added keys
	for key2, val2 := range map2 {
		if _, ok := map1[key2]; !ok {
			newPath := buildPath(path, key2)
			diffs = append(diffs, Difference{
				Type:   BodyValueAdded,
				Path:   newPath,
				NewVal: formatValue(val2, 50),
			})
		}
	}

	return diffs
}

// compareArrays diffs two JSON arrays (order-independent).
func compareArrays(
	path string,
	arr1, arr2 []any,
	ignoredPaths map[string]struct{},
	currentDepth int,
) []Difference {
	var diffs []Difference
	if len(arr1) != len(arr2) {
		diffs = append(diffs, Difference{
			Type:   ArrayLengthChanged,
			Path:   path,
			OldLen: len(arr1),
			NewLen: len(arr2),
		})
	}

	// This is the O(N^2) Go equivalent of Rust's HashSet-based diff.
	// It finds elements in one array that are not DeepEqual to any in the other.
	matches1 := make([]bool, len(arr1))
	matches2 := make([]bool, len(arr2))

	for i, el1 := range arr1 {
		for j, el2 := range arr2 {
			if !matches2[j] && reflect.DeepEqual(el1, el2) {
				matches1[i] = true
				matches2[j] = true
				break
			}
		}
	}

	// Report unmatched elements from arr1 as "removed"
	for i, matched := range matches1 {
		if !matched {
			diffs = append(diffs, Difference{
				Type:   ArrayElementRemoved,
				Path:   fmt.Sprintf("%s[*]", path),
				OldVal: formatValue(arr1[i], 50),
			})
		}
	}

	// Report unmatched elements from arr2 as "added"
	for j, matched := range matches2 {
		if !matched {
			diffs = append(diffs, Difference{
				Type:   ArrayElementAdded,
				Path:   fmt.Sprintf("%s[*]", path),
				NewVal: formatValue(arr2[j], 50),
			})
		}
	}

	return diffs
}

// --- Diff Helpers ---

func buildPath(base, key string) string {
	if base == "" {
		return key
	}
	return base + "/" + key
}

func formatValue(value any, maxLength int) string {
	switch v := value.(type) {
	case string:
		if len(v) > maxLength {
			return fmt.Sprintf(`"%s..."`, v[:maxLength])
		}
		return fmt.Sprintf(`"%s"`, v)
	case []any:
		return fmt.Sprintf("Array[%d]", len(v))
	case map[string]any:
		return fmt.Sprintf("Object{%d keys}", len(v))
	default:
		s := fmt.Sprintf("%v", v)
		if len(s) > maxLength {
			return s[:maxLength] + "..."
		}
		return s
	}
}
