# Release Sanity Checker ⚠️🤖🦾

This tool helps you ensure the consistency of your API responses over time. It fetches responses for a set of pre-defined requests, compares them against previous responses stored in a database, and reports any differences.  This is particularly useful for regression testing and ensuring that API changes don't introduce unexpected behavior.

## 🚀 How to run

- **Download binary**  
Download your preferred version from [releases](https://github.com/drew458/release-sanity-checker/releases) and just run `release-sanity-checker <FILENAME>`.
 
## ▶️ Usage

```bash
release-sanity-checker [options] <config_path>
```

### 🕹️ Options

    --file <config_path>: Run with a specific config file (default mode).
    --directory <dir_path>: Run with all config files found in the directory.
    --ignore-headers: Do not look for changes in response headers.
    --baseline: Build the baseline for the requests. This will overwrite existing responses in the database with the current responses.
    --verbose: Print the full response body/header when changed and response that didn't change.

### 🚦 Examples

- **Build the baseline**

```bash
release-sanity-checker --baseline config.json
release-sanity-checker --baseline --directory examples
```

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

## ✅ Configuration File Format

**Requests object**

The configuration file is a JSON file containing an array of request definitions. Each request definition must have the following fields:

| Name | Type | Mandatory | Description | 
|---|---|---|---|
| id | String | Y | A unique identifier for the request |
| flow | Array | Y | The HTTP requests to run. Only the last one will be checked for differences in the response |
| ignore_paths | Array | N | A list of path to ignore in the response when checking for differences |

**Flow object**

| Name | Type | Mandatory | Description | 
|---|---|---|---|
| url | String | Y | The URL to make the request to |
| headers | Object | N | A map of headers to include in the request |
| body | Object | N | The request body (can be any valid JSON value) |


```JSON
{
    "requests": [
        {
            "id": "1",
            "flow": [
                {
                    "url": "https://api.restful-api.dev/objects",
                    "headers": {
                        "Content-Type": "application/json"
                    },
                    "body": {
                        "name": "Apple MacBook Pro 16",
                        "data": {
                            "year": 2019,
                            "price": 1849.99,
                            "CPU model": "Intel Core i9",
                            "Hard disk size": "1 TB"
                        }
                    }
                },
                {
                    "url": "https://api.restful-api.dev/objects",
                    "headers": {
                        "Content-Type": "application/json"
                    },
                    "body": {
                        "name": "Apple MacBook Pro 16",
                        "data": {
                            "year": 2019,
                            "price": 1849.99,
                            "CPU model": "Intel Core i9",
                            "Hard disk size": "1 TB"
                        }
                    }
                }
            ],
            "ignore_paths": ["/createdAt", "/id"]
        },
        {
            "id": "2",
            "flow": [
                {
                    "url": "https://api.restful-api.dev/objects",
                    "headers": {
                        "Content-Type": "application/json"
                    },
                    "body": {
                        "name": "Apple MacBook Pro 16",
                        "data": {
                            "year": 2019,
                            "price": 1849.99,
                            "CPU model": "Intel Core i9",
                            "Hard disk size": "1 TB"
                        }
                    }
                }
            ]
        }
    ]
}
```
