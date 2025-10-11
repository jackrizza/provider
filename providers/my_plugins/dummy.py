class Provider:
    def name(self):
        return "dummy_py"

    def fetch_entities(self, payload):
        # payload is a dict matching your EntityInProvider JSON
        # return a list of Entities (as dicts) that match your Rust Entity struct
        return [{
            "id": "dummy_py:TEST:2025-09-01..2025-09-10",
            "source": "dummy_py",
            "tags": "[\"ticker=TEST\",\"from=2025-09-01T00:00:00Z\",\"to=2025-09-10T00:00:00Z\"]",
            "data": "[{\"timestamp\":\"2025-09-01T00:00:00Z\",\"value\":14},{\"timestamp\":\"2025-09-02T00:00:00Z\",\"value\":2},{\"timestamp\":\"2025-09-03T00:00:00Z\",\"value\":3},{\"timestamp\":\"2025-09-04T00:00:00Z\",\"value\":4},{\"timestamp\":\"2025-09-05T00:00:00Z\",\"value\":5}]",
            "etag": "deadbeef",
            "fetched_at": "2025-09-10T00:00:00Z",
            "refresh_after": "2025-09-11T00:00:00Z",
            "state": "ready",
            "last_error": "",
            "updated_at": "2025-09-10T00:00:00Z"
        }]