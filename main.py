import json
import logging
import tempfile
from configparser import ConfigParser

import requests as requests

# Read config file
config = ConfigParser()
config.read('config.ini')

# Initialize logging format
logging.basicConfig(format='[%(process)d - %(processName)s] [%(thread)d - %(threadName)s] [%(levelname)s] %(asctime)s '
                           '- %(message)s', level=logging.INFO)


def call_microservice(payload):
    """
    Perform a request to the endpoint specified by the 'url' parameter with the given 'payload' as body
    :param payload: A dict that contains 'url' and 'payload' as keys
    :return: The JSON formatted response
    """
    logging.info("Sending request to %s", payload['url'])
    request = requests.post(payload['url'], json=payload['payload'])

    return request.json()


def main():
    print('Welcome to the release sanity checker. This tool allows you to check for differences in endpoint responses '
          'before and after a release. Now it will start to collect responses to the configured endpoints (head to '
          'config.ini file to know more)\n')
    env = input('Insert environment to run the query on: (svil, test, prod):\n').lower()

    if env not in ('svil', 'test', 'prod'):
        return

    with tempfile.TemporaryFile(mode='r+', encoding='utf-8') as request_compare_tmp, \
            tempfile.TemporaryFile(mode='r+', encoding='utf-8') as response_compare_tmp:

        path_items = config.items('urls-' + env)

        for microservice_name, url in path_items:
            try:
                endpoints = [v.strip() for v in config.get(microservice_name, 'endpoints').split(',')]
            except Exception:
                continue

            for endpoint in endpoints:
                request_file_path = 'requests' + endpoint + '.json'
                response_file_path = 'responses' + endpoint + '.json'
                with open(request_file_path, "r", encoding='utf-8') as request, \
                        open(response_file_path, "r", encoding='utf-8') as expected_response_file:

                    payload = {'url': url + endpoint, 'payload': json.load(request)}
                    response_json = call_microservice(payload)

                    request_compare_tmp.truncate()
                    response_compare_tmp.truncate()
                    request_compare_tmp.write(json.dumps(response_json, indent=4))
                    response_compare_tmp.write(json.dumps(json.load(expected_response_file), indent=4))

                    request_compare_tmp.flush()
                    response_compare_tmp.flush()

                    for line1, line2 in zip(request_compare_tmp, response_compare_tmp):
                        if line1 != line2:
                            print(f"Found difference: {line1}")


if __name__ == '__main__':
    main()
