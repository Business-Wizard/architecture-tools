class StripeClient:
    def __init__(self, api_key: str) -> None:
        self.api_key = api_key

    def charge(self, amount_cents: int, customer_email: str) -> str:
        print(f"Charging {customer_email} {amount_cents} cents")
        return "ch_stub_ok"

    def refund(self, charge_id: str) -> bool:
        print(f"Refunding charge {charge_id}")
        return True
