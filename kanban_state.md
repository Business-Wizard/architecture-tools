ideas:
- ETA, based on measured Throughput or Cycle Time. 70% confidence interval should do fine
- pivot to object level analysis. faster, no tests/types, object level granularity
- Abstractness metric that accounts for semantic abstraction objects. eg Entities, Value Objects, concrete method count.
  - interfaces + semantic abstractions (domain) / objects_count + concrete_methods_count
- cache on mutation results, invalidate based on app or src/ changes.
  - separate per repo, fast and mutation mode
  - sqlite?
- improve report
  - "all" report
  - "top-3" report
  - default is "top-3" report
  - "not files, but trends"? eg test sizes, coverage
  - LLM suggested refactors
  - show incremental change since last run
  - show next top action items
- quickly scan a github repo of interest for a score/report. browser WASM, easy delivery to user?

user feedback:
- report of historical metrics over time, to see time trends. CI oriented, weekly resolution.
- "something like ntops or btops"?
- "distance metric isn't valuable, actionable"
- add ROI "so what" justification. eg SOLID principles, time savings, etc
- add VCS data to augment report? eg see stability thru change frequency view.
- add CI outcomes or Change Failure Rate? to build ROI for improving architecture of modules or objects.

tech debt:
- rm mod.rs, old convention
- test case `test_given_composite_usecase_should_measure_abstractness_above_zero`

done:
