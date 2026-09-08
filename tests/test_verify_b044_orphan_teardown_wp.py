#!/usr/bin/env python3
import unittest

from scripts.verify_b044_orphan_teardown_wp import MUTATIONS, verify_mutations, verify_repo


class B044ContractTests(unittest.TestCase):
    def test_repository_contract_is_complete(self):
        verify_repo()

    def test_all_safety_mutations_fail_closed(self):
        self.assertEqual(verify_mutations(), len(MUTATIONS))


if __name__ == "__main__":
    unittest.main()
