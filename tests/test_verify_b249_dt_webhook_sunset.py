#!/usr/bin/env python3
import unittest

from scripts.verify_b249_dt_webhook_sunset import MUTATIONS, verify_mutations, verify_repo


class B249SunsetContractTests(unittest.TestCase):
    def test_contract_is_complete(self):
        verify_repo()

    def test_mutations_fail_closed(self):
        self.assertEqual(verify_mutations(), len(MUTATIONS) + 1)


if __name__ == "__main__":
    unittest.main()
