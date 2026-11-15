# NIGO 엔진 개발 후보 nigo-engine-v0.1.0-rc.4

- source `037c98c315d5322fa7da266ca0254d61f3863622`, dirty=false. JAR `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a`, 164133636 bytes.
- 검증한 동일 bytes의 공급자 묶음이며, 원격 게시·BXDL 수신·제품 인수 완료를 뜻하지 않습니다.
- 예상 private prerelease: https://github.com/bwg-loonshots/nigo-protocol/releases/tag/nigo-engine-v0.1.0-rc.4
- `development`, `officialRelease=false`, 계약 `PROPOSED`와 원본 manifest를 유지합니다.
- 자체 라이선스 조건 `UNDECIDED`, 고객 납품 허가 `NOT_GRANTED`입니다.

새 수신 디렉터리에서 trusted archive SHA/크기와 전체 파일 목록을 검증하십시오.
digest 디렉터리의 `shasum -a 256 -c SHA256SUMS` 및 선정 Java 21의
`java -jar nigo-node-0.0.1-SNAPSHOT.jar engine-info` 전체 identity를 `evidence/engine-info.json`과 비교하십시오.
기존 RC.1/RC.2/RC.3 bytes/tag/asset/BXDL lock을 자동 교체하지 않습니다.

## 검증 범위

G1 source 회귀, G2 supervised canonical, G3 선정 JRE CLI9/Storage10 및 full Console mock,
G4 여섯 exact-JAR runner, G5 fresh candidate 두 경로 및 H2/RocksDB restart/observer/import를 구분합니다.
실제 건수·named opt-in 미실행·정확한 근거 digest는 `evidence/`에 기록합니다.
RC.1 forward, 기존 DB migration, mixed-binary rolling upgrade를 검증했다고 주장하지 않습니다.
mock UI는 Java API 또는 BXDL package 인수의 대체 증거가 아닙니다.

계약 snapshot 4개와 `PROVIDERS.md`, `ROCKSDB_OPERATIONS.md`, `STORAGE_INTEGRITY.md`를 포함합니다.
복구 전 DB 사용자를 멈추고 명시 대상·의도를 확인하며, UNKNOWN/실패 시 자동 repair/재시작·WAL 삭제를 하지 마십시오.
raw XML/log/stderr, Playwright trace/profile, 시험 실행기, config/key/password/PKI/transaction/DB/WAL,
source archive 및 로컬 홈 경로는 제외했습니다. 이전 실패 근거는 삭제하거나 최종 PASS로 덮어쓰지 않습니다.
BXDL의 실제 수신·명시 lock 선택·package/JRE/service 재인수와 공급망/재배포 권한은 별도입니다.
