from src.customer import Customer


def test_customer_creation_should_store_id() -> None:
    customer = Customer("c1", "John Doe", "john@example.com")
    assert customer.customer_id == "c1"


def test_customer_creation_should_store_name() -> None:
    customer = Customer("c1", "John Doe", "john@example.com")
    assert customer.name == "John Doe"


def test_customer_creation_should_store_email() -> None:
    customer = Customer("c1", "John Doe", "john@example.com")
    assert customer.email == "john@example.com"


def test_get_full_name_should_return_name() -> None:
    customer = Customer("c1", "Jane Smith", "jane@example.com")
    actual = customer.get_full_name()
    assert actual == "Jane Smith"


def test_update_email_should_change_email() -> None:
    customer = Customer("c1", "John Doe", "old@example.com")
    customer.update_email("new@example.com")
    assert customer.email == "new@example.com"
