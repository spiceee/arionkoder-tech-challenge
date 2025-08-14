# arionkoder-tech-challenge

## 1. JSON Stream Processor

Write a Rust program that:
- Receives a continuous stream of JSON messages.
- Extracts relevant fields from each JSON message.
- Writes the extracted data to a file.

Constraints:
- Efficiently handle incoming messages without increasing memory usage significantly.
- Handle possible malformed JSON gracefully.

## Assumptions

Since the JSON messages are continuous and potentially large, we assume that the program will be designed to handle them in a streaming manner.
The input format then is a proto-JSON thing, where every new line is a JSON object in itself, and not a single JSON array (where we would have to wait until the end of the array to process it).

I feel odd about tasks that in themselves could not be a JIRA task. Like, the specs of the input schema and how large it is expected to be should be defined prior to implementation.
