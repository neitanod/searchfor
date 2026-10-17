class Outer:
    def method(self):
        def inner():
            return "needle"
        return inner()

    async def fetch(self):
        return "needle"


def top():
    return "needle"
