import unittest
from tools.verify_test_inventory import compare_suite, libtest_names


class InventoryTests(unittest.TestCase):
    def test_names_keep_module_identity_and_ignore_summary_lines(self):
        self.assertEqual(libtest_names('a::one: test\nb::one: test\n\n2 tests, 0 benchmarks\n'),{'a::one','b::one'})

    def test_missing_ignored_or_filtered_tests_cannot_pass(self):
        cases={'a':{'ignored':False,'filter-match':{'status':'matches'}},
               'b':{'ignored':True,'filter-match':{'status':'mismatch'}}}
        compare_suite(cases,{'a','b'},{'b'})
        for all_names,ignored in [({'a'},set()),({'a','b'},set()),({'a','b','c'},{'b'})]:
            with self.assertRaises(ValueError): compare_suite(cases,all_names,ignored)
        cases['a']['filter-match']['status']='mismatch'
        with self.assertRaises(ValueError): compare_suite(cases,{'a','b'},{'b'})


if __name__=='__main__': unittest.main()
