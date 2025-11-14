package main

import (
	"fmt"

	"github.com/fatih/color"
)

// PrintMessage is the message format sent to the printer goroutine.
type PrintMessage struct {
	RequestID   string
	Differences []Difference
}

// runDifferencesPrinter is the actor function that runs in its own goroutine.
func runDifferencesPrinter(msgChan <-chan PrintMessage, doneChan chan<- struct{}) {
	for msg := range msgChan {
		red := color.New(color.FgRed).SprintFunc()
		green := color.New(color.FgGreen).SprintFunc()
		yellow := color.New(color.FgYellow).SprintFunc()
		white := color.New(color.FgWhite, color.Bold).SprintFunc()

		fmt.Println(yellow("\n❌-----------------------------------------------------------------------------------------❌"))
		fmt.Println(yellow(fmt.Sprintf("Differences detected for request with ID: '%s'", msg.RequestID)))

		for _, diff := range msg.Differences {
			switch diff.Type {
			case StatusCodeChanged:
				fmt.Println("  Status Code Difference:")
				fmt.Printf("    - %s\n", green(diff.OldVal))
				fmt.Printf("    + %s\n", red(diff.NewVal))
			case HeaderValueChanged:
				fmt.Printf("    Changed Header: %s\n", white(diff.HeaderName))
				fmt.Printf("      - %s\n", green(diff.OldVal))
				fmt.Printf("      + %s\n", red(diff.NewVal))
			case HeaderValueRemoved:
				fmt.Printf("    Removed Header: %s\n", white(diff.HeaderName))
			case HeaderValueAdded:
				fmt.Printf("    Added Header: %s\n", white(diff.HeaderName))
			case BodyValueChanged:
				fmt.Printf("    Changed body value at '%s'\n", white(diff.Path))
				fmt.Printf("      - %s\n", green(diff.OldVal))
				fmt.Printf("      + %s\n", red(diff.NewVal))
			case BodyValueRemoved:
				fmt.Printf("    Removed body value at '%s'\n", white(diff.Path))
				fmt.Printf("      - %s\n", green(diff.OldVal))
			case BodyValueAdded:
				fmt.Printf("    Added body value at '%s'\n", white(diff.Path))
				fmt.Printf("      + %s\n", red(diff.NewVal))
			case ArrayLengthChanged:
				fmt.Printf("    Array length changed at '%s'\n", white(diff.Path))
				fmt.Printf("      - length: %s\n", green(diff.OldLen))
				fmt.Printf("      + length: %s\n", red(diff.NewLen))
			case ArrayElementRemoved:
				fmt.Printf("    Array element removed at '%s'\n", white(diff.Path))
				fmt.Printf("      - %s\n", green(diff.OldVal))
			case ArrayElementAdded:
				fmt.Printf("    Array element added at '%s'\n", white(diff.Path))
				fmt.Printf("      + %s\n", red(diff.NewVal))
			case DifferentBodyString:
				fmt.Println("\n  Body (non-JSON or invalid JSON):")
				fmt.Printf("    - %s\n", green(truncateString(diff.OldVal, 100)))
				fmt.Printf("    + %s\n", red(truncateString(diff.NewVal, 100)))
			}
		}
		fmt.Println(yellow("❌-----------------------------------------------------------------------------------------❌"))
	}
	// Signal that all messages have been processed
	doneChan <- struct{}{}
}

func truncateString(s string, length int) string {
	if len(s) <= length {
		return s
	}
	return s[:length] + "..."
}
