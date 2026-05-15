from decimal import Decimal

from src.repository import save_order, find_order


def test_save_order_should_persist_and_confirm() -> None:
    order = save_order("o30", "c30", "Dave", "dave@example.com", Decimal("50.00"))
    assert order.status == "confirmed"


def test_find_order_should_return_saved_order() -> None:
    save_order("o31", "c31", "Eve", "eve@example.com", Decimal("75.00"))
    found = find_order("o31")
    assert found is not None and found.order_id == "o31"
