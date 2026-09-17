# NIGO 개발 후보 확인과 cold 검사

NIGO의 `engine-info`와 `preflight`를 별도 JVM에서 호출한다. `init`, `resume-init`, `run`과 서비스 관리는 아직 연결하지 않는다. 실제로 읽은 공급 계약은 [2026-09-17 snapshot](../contracts/nigo/development-2026-09-17/README.md)이며 PROPOSED/development다.

```bash
bxdl engine inspect --jar /absolute/package/engine/nigo-node.jar \
  --java /absolute/package/runtime/bin/java --lock ./trusted-engine.lock.json \
  --allow-development --json

bxdl engine preflight --jar /absolute/package/engine/nigo-node.jar \
  --java /absolute/package/runtime/bin/java --lock ./trusted-engine.lock.json \
  --config /absolute/instance/node.json --allow-development --json
```

`--lock`은 패키지 밖에서 신뢰한 입력이다. 형식은 `contracts/bxdl/engine-lock.schema.json`이며 다음 값을 모두 담는다.

- `schemaVersion=1`, `jarSha256`, `jarSizeBytes`, `javaSha256`.
- `expected`: NIGO engine-info 전체 DTO. AVAILABLE인 buildInfoStatus와 engine/version/source/java/console/contract/distribution을 고정한다. source.dirty도 정확히 일치해야 한다.

`packaging/engine-lock.development.example.json`은 이번 수신 JAR의 expected identity 예시다. Java SHA-256은 0으로 비워 두었으므로 그대로 실행할 수 없다. 신뢰해서 선택한 Java executable의 SHA-256으로 채우고, 다른 후보를 소비할 때는 전체 identity를 새 근거로 검토한다.

제공자 manifest에서 expected 부분을 가져오되 source·JAR hash·계약 fingerprint를 제공자 기록 및 내장 자료와 교차 확인한다. 내려받은 파일의 hash를 그 자리에서 믿는 것만으로 공급자가 인증되지 않는다. 이 저장소는 사용자 호스트 Java hash를 공식 제품 lock으로 배포하지 않는다. Java 실행파일 하나의 hash는 전체 JRE 의존 파일의 인증이 아니며, 제품 패키지의 전체 inventory 검증과 정식 runtime 선정은 별도다.

실행 전 JAR를 0700 임시 폴더로 복사하면서 기대 size/hash를 확인한다. JVM 옵션·환경변수는 상속하지 않고 stdin을 닫는다. 출력 크기와 실행 시간을 제한하며 timeout은 직접 child를 종료·회수한 뒤 UNKNOWN/exit 6으로 보고한다. `--timeout-seconds`는 1~120, 기본 30으로 각 JVM 호출에 적용한다. 프로세스 원문 stderr나 잘못된 JSON을 사용자 결과에 되돌려 쓰지 않는다. 예상한 identity와 실제 응답이 일치해야 다음 검사로 진행한다. 검토해서 신뢰한 개발 후보만 실행하며, 이 wrapper는 동일 UID의 공격자나 임의 자손 프로세스를 격리하는 sandbox가 아니다.

`--config`는 **NIGO native node.json**이다. setup이 내보내는 BXDL instance.json과 다르다. node.json의 chainFile/dataDirectory/backend/node, chain의 flat property와 peer/mTLS 자료는 NIGO 계약에 맞게 별도 준비한다. 현재 제품 설정에 없는 validator ID·peer/pin·genesis 정보를 BXDL이 만들어 넣지 않는다. config/공개 chain은 snapshot으로 고정하고 상대 경로는 원래 config 위치 기준 의미를 유지한다. 키 내용·canonical 검증은 NIGO가 담당한다.

성공한 engine-info는 exit 0이다. 올바른 cold 검사도 NIGO exit 3/INCOMPLETE를 반환하며 BXDL은 **exit 5/INCOMPLETE**로 보존한다. CONFIGURATION·KEY_MATERIAL의 실제 결과를 받고 DATABASE_AND_WAL·PORTS_AND_PEERS·NATIVE_RUNTIME의 NOT_CHECKED를 READY로 올리지 않는다. INSTANT smoke의 KEY_MATERIAL=NOT_APPLICABLE은 QBFT key/mTLS 인수가 아니다.

현재 소비한 후보는 source dirty=true인 개발 빌드다. 확인 중 NIGO가 새 종료·live sync 보완을 작업 트리에 작성하는 것을 관측했지만, 그 변경의 새 canonical JAR·시험 근거는 이번 소비 대상에 포함되지 않았다. QBFT는 정상 신호 종료에도 `UNKNOWN / QBFT_CLEANUP_NOT_FULLY_OBSERVED`일 수 있으며 live follower 공급자 단절 관측·source 재선택과 genesis/bootstrap sync 경계도 남아 있다. 따라서 이번 cold 연결을 자동 시작/재시작·정상 종료·동일 DB 재시작 또는 G1-M 전체 완료로 취급하지 않는다.
