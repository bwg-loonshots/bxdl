# NIGO 엔진 개발 후보 nigo-engine-v0.1.0-rc.3

- source `7ec5abd8c98e1cf070763a16f815e9281b74a5c1`, dirty=false; JAR `05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f`, 164088530 bytes.
- 이 묶음은 검증한 동일 bytes의 공급자 자료입니다. 원격 게시·BXDL 수신/제품 인수 완료를 뜻하지 않습니다.
- 예상 private prerelease: https://github.com/bwg-loonshots/nigo-protocol/releases/tag/nigo-engine-v0.1.0-rc.3
- `development`, `officialRelease=false`, 계약 `PROPOSED`와 원본 manifest 상태를 유지합니다.
- 자체 라이선스 조건 `UNDECIDED`, 고객 납품 허가 `NOT_GRANTED`를 유지합니다.

새 수신 디렉터리에서 trusted archive hash/크기와 전체 파일 목록을 확인하고, 이 digest 디렉터리의
`shasum -a 256 -c SHA256SUMS` 및 선정 Java 21의 `java -jar nigo-node-0.0.1-SNAPSHOT.jar engine-info` 전체 identity를
`evidence/engine-info.json`과 비교하십시오. 기존 RC.1/RC.2 bytes/tag/asset/lock을 자동 교체하지 않습니다.

## 근거와 운영 범위

canonical 1,299건(1,297 PASS, 기존 opt-in 2 NOT_RUN)은 packaged Engine9/Storage8을 포함합니다.
별도 선정 Temurin Engine9/Storage10, Console mock69, 유한 candidate/RC.1 forward 복구를 구분합니다.
Storage의 managed same-JAR 재시작 검증과 source-classpath fixture 검증을 혼동하지 않습니다.
후속 supplier 결과는 `evidence/`의 고정 JAR 연결 자료이며 원본 manifest를 수정하지 않았습니다.

`nigo-java/nigo-node/STORAGE_INTEGRITY.md`의 inspect/repair/exit·JSON/불명확 결과 계약을 따르십시오.
복구 전 모든 DB 사용자를 멈추고 명시 대상·의도를 확인하며, UNKNOWN/실패에는 자동 repair/재시작·WAL 삭제를 하지 않습니다.
`ENGINE_CONTRACT.md`, `ENGINE_RUNTIME_CONTRACT.md`, `contract.json`, `health-cases.json`은 이 JAR의 계약 snapshot입니다.
`PROVIDERS.md`, `ROCKSDB_OPERATIONS.md`, `STORAGE_INTEGRITY.md`는 고정 source의 운영 문서입니다.

raw XML/log/child stderr·Playwright trace/profile·시험 실행기·config/keys/password/PKI/DB/WAL·source archive는 제외했습니다.
BXDL의 실제 수신·명시 lock 선택·A7 wrapper·선정 package/JRE/service 재인수와 공급망/재배포 권한은 별도입니다.
기존 FAIL 증거는 독립 이력으로 보존하며 최종 PASS로 덮어쓰지 않았습니다.
