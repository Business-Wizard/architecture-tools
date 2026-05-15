from src.routes import post_order


def test_post_order_should_return_confirmed_status() -> None:
    result = post_order("o10", "c10", "Alice", "alice@example.com", "29.99")
    assert result == {"order_id": "o10", "status": "confirmed"}
