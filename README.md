# Release Sanity Checker

This tool helps you ensure the consistency of your API responses over time. It fetches responses for a set of pre-defined requests, compares them against previous responses stored in a database, and reports any differences.  This is particularly useful for regression testing and ensuring that API changes don't introduce unexpected behavior.

## How to run

- **Download binary**  
Download your preferred version from [releases](https://github.com/drew458/release-sanity-checker/releases) and just run `release-sanity-checker <FILENAME>`.

- **Container**  
Assuming you have Podman or Docker installed on your machine:
    - ```bash
      podman build -t release-sanity-checker .
      ```
    - ```bash
      podman run -v "<FILENAME>:/app/config.json" -v release-sanity-checker /app/config.json
      ```
    
    If you want to run it against a directory, use:  
    - ```bash
      podman run -v "<FILENAME>:/app/examples" -v release-sanity-checker --directory /app/examples
      ```
 
## Usage

```bash
release-sanity-checker [options] <config_path>
```

### Options

    --file <config_path>: Run with a specific config file (default mode).
    --directory <dir_path>: Run with all config files found in the directory.
    --ignore-headers: Do not look for changes in response headers.
    --baseline: Build the baseline for the requests. This will overwrite existing responses in the database with the current responses.
    --help: Display this help message.

### Examples

- **Run with a specific config file**

```bash
release-sanity-checker config.json
release-sanity-checker --file config.json
```

- **Run with all .json files in a directory**

```bash
release-sanity-checker --directory examples
```

- **Ignore header changes**

```bash
release-sanity-checker --file config.json --ignore-headers
```

- **Build a new baseline**

```bash
release-sanity-checker --file config.json --baseline
```

## Configuration File Format

The configuration file is a JSON file containing an array of request definitions. Each request definition must have the following fields:

| Name | Mandatory | Description | 
|---|---|---|
| id | Y | A unique identifier for the request |
| url | Y | The URL to make the request to |
| headers | N | A map of headers to include in the request |
| body | N | The request body (can be any valid JSON value) |

```JSON

{
  "requests": [
    {
      "id": "get_objects",
      "url": "[https://api.example.com/objects](https://www.google.com/search?q=https://api.example.com/objects)",
      "headers": {
        "Content-Type": "application/json"
      }
    },
    {
      "id": "create_object",
      "url": "[https://api.example.com/objects](https://www.google.com/search?q=https://api.example.com/objects)",
      "headers": {
        "Content-Type": "application/json"
      },
      "body": {
        "name": "New Object",
        "value": 123
      }
    }
  ]
}
```

## Database

The tool uses a SQLite database file named `release-sanity-checker-data.db` to store previous responses. This file is created in the current working directory.
