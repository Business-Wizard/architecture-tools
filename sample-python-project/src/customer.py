"""Customer domain model."""


class Customer:
    """Represents a customer in the system."""

    def __init__(self, customer_id: str, name: str, email: str) -> None:
        self.customer_id = customer_id
        self.name = name
        self.email = email

    def get_full_name(self) -> str:
        """Return the customer's full name."""
        return self.name

    def update_email(self, email: str) -> None:
        """Update the customer's email address."""
        self.email = email
