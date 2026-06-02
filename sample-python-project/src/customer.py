"""Customer domain model."""


class Customer:
    """Represents a customer in the system."""

    def __init__(self, customer_id: str, name: str, email: str) -> None:
        self.customer_id = customer_id
        self.name = name
        self.email = email
