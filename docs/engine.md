# 제품 설정과 NIGO 개발 후보의 cold 검사

setup으로 저장한 제품 설정과 운영자가 준비한 NIGO native 설정이 같은 노드를 가리키는지 확인하고, 고정한 개발 후보의 cold 검사를 실행할 수 있다. `init`, `resume-init`, `run`과 서비스 관리는 아직 연결하지 않는다. 최신 수신 기준은 [clean 개발 후보 snapshot](../contracts/nigo/development-clean-2026-09-18/README.md)이며 계약은 `PROPOSED`, 채널은 `development`, `officialRelease=false`다.

## 제품 설정과 함께 검사하기

먼저 [setup](./setup.md)에서 `instance.json`을 내보내고, [파일 설치](./install.md)로 선택한 패키지 또는 별도로 검토한 JAR·Java 21을 준비한다. 설치 receipt는 실행할 엔진을 신뢰하는 lock을 대신하지 않는다.

```bash
bxdl preflight --config ./instance.json \
  --engine-config /absolute/instance/node.json \
  --jar /absolute/package/engine/nigo-node.jar \
  --java /absolute/package/runtime/bin/java \
  --lock ./trusted-engine.lock.json --allow-development --json
```

`--engine-config`, `--jar`, `--java`, `--lock`, `--allow-development`는 모두 함께 지정한다. `--timeout-seconds`는 이 결합 검사에서만 추가할 수 있고 1~120, 기본 30이다. 엔진 옵션을 모두 생략한 `bxdl preflight --config ./instance.json`은 기존 로컬 metadata 검사만 수행한다.

순서는 제품 설정·로컬 metadata 확인, 제품/native 설정의 대응 값 비교, JAR·Java·engine-info identity 검증, NIGO cold 검사다. 로컬 검사에 FAIL이 있으면 엔진을 실행하지 않고 exit 4를 반환한다. 설정이 서로 다르거나 필요한 대응 값이 없으면 `ENGINE_PRODUCT_MISMATCH`/exit 3으로 중단한다. 결합 검사의 제품 schema/read 등 입력 오류는 exit 3이다. 엔진 옵션 없는 기존 로컬 검사의 schema/read 오류는 exit 2를 유지한다.

JSON 결과의 `data.product`는 로컬 검사 결과, `data.configurationBinding`은 `NOT_CHECKED` 또는 `MATCHED`, 선택 `data.engine`은 실제 engine 검사 결과다. 결합 보고서의 `data.outcome`은 `FAIL` 또는 `INCOMPLETE`다. `MATCHED`는 두 설정의 대응 값이 일치했다는 뜻이며 peer 연결·합의·DB 상태의 판정이 아니다. 실패가 오류 envelope로 반환되면 성공한 결합 보고서가 있다고 가정하지 않는다.

제품의 14개 입력 중 instanceId는 제품 전용 이름이다. 나머지 입력과 고정 role/backend를 native의 node identity·HTTP/P2P·data/chain·secret 참조와 대조한다. QBFT, VALIDATOR, MTLS를 명시한 native 입력만 결합하며 INSTANT smoke나 observer 설정을 validator 제품 설정의 검사로 사용하지 않는다. 대응 값의 누락을 엔진 개발 기본값으로 채우지 않는다. 정확한 매핑은 [결합 설계](../design/2026-09-18-product-engine-preflight.md)를 따른다.

## 두 종류의 설정과 필요한 자료

`instance.json`은 BXDL 제품 입력이고 `node.json`은 NIGO native 입력이다. setup은 native 설정을 자동 생성하지 않는다. node.json의 `chainFile`, `dataDirectory`, `backend`, `node`와 공개 chain의 flat properties는 [수신 계약](../contracts/nigo/development-clean-2026-09-18/ENGINE_CONTRACT.md)에 맞게 별도 준비한다. 각 설정의 상대경로는 각 원본 설정 파일 위치를 기준으로 해석한다.

공개 node ID는 `0x`로 시작하는 32-byte hex이고 validator ID와 다르다. 제품 v1의 일반 식별자나 예제 placeholder가 이 엔진 identity를 대신하지 않는다. 제품에 없는 validator ID, peer 목록·역할·거래 권한·credential pin, sync/ingress 정책 등은 native 입력으로 제공하며 NIGO가 검증한다. BXDL이 chain membership·genesis나 PKI를 추측해 생성하지 않는다.

Validator keystore는 NIGO의 **NGVK revision-1 binary** 형식이다. TLS key/trust store의 PKCS12/JKS와 다르며 파일 확장자만 바꿔 변환되지 않는다. 기존 제품 예제의 validator-keystore `.p12` 이름은 경로 placeholder일 뿐 호환 형식의 근거가 아니다. TLS store type은 native 설정과 NIGO 검증을 따른다.

Peer credential pin은 NIGO의 도메인 분리 fingerprint다. 일반 인증서 SHA-256 또는 SPKI hash를 그대로 넣지 않는다. 발급·배치 담당이 준비한 NIGO 형식의 pin을 사용한다. 구체적인 입력 규칙과 형식 구분은 [결합 설계](../design/2026-09-18-product-engine-preflight.md)에 정리한다.

BXDL 제품 작업이 source-free QBFT provisioning과 거래·동일 finalized 높이 비교 fixture의 입력 준비를 맡는다. 현재 CLI에는 해당 생성 기능이 없다. 운영에 사용할 chain/membership·signer/password·mTLS trust/pin 자료는 별도 준비가 필요하다. 공급자의 소스 기반 generator 시험이나 INSTANT 예제를 source-free QBFT 제품 인수로 표시하지 않는다.

## 엔진만 독립 검사하기

제품 설정 없이 수신 자료를 확인할 때는 기존 명령을 사용한다. 이 명령들은 제품 설정과의 일치를 확인하지 않는다.

```bash
bxdl engine inspect --jar /absolute/package/engine/nigo-node.jar \
  --java /absolute/package/runtime/bin/java --lock ./trusted-engine.lock.json \
  --allow-development --json

bxdl engine preflight --jar /absolute/package/engine/nigo-node.jar \
  --java /absolute/package/runtime/bin/java --lock ./trusted-engine.lock.json \
  --config /absolute/instance/node.json --allow-development --json
```

`engine preflight --config`는 NIGO node.json, `preflight --config`는 BXDL instance.json을 받는다. 독립 엔진 명령의 INSTANT smoke 검사는 제품 QBFT 검사와 구별한다.

## lock과 실행 경계

`--lock`은 패키지 밖에서 신뢰한 입력이다. [schema](../contracts/bxdl/engine-lock.schema.json)는 `schemaVersion=1`, `jarSha256`, `jarSizeBytes`, `javaSha256`과 `expected`를 요구한다. `expected`는 AVAILABLE인 buildInfoStatus 및 engine/version/source/java/console/contract/distribution 전체 engine-info DTO를 고정한다. source.dirty도 정확히 일치해야 한다.

[Clean lock 예시](../packaging/engine-lock.clean-development.example.json)는 source `303e163a9b3f293fa39e42d02b8daa1843973c14`, dirty=false인 새 공급 JAR의 identity다. Java SHA-256은 0으로 남겨 두었으므로 그대로 실행할 수 없다. 검토해 선택한 Java executable의 hash로 채운다. [기존 lock 예시](../packaging/engine-lock.development.example.json)는 source `aee1cbe5…`, dirty=true인 이전 후보의 이력이다. 새 JAR에 기존 source·hash·계약 fingerprint나 시험 근거를 재사용하지 않는다.

제공자 manifest·인계 기록·JAR 내장 자료를 교차 확인한다. 전달받은 파일의 hash를 그 자리에서 믿는 것만으로 공급자가 인증되지 않는다. Java 실행파일 하나의 hash는 전체 JRE 의존 파일의 인증이 아니며, 제품 패키지 전체 inventory 검증과 정식 runtime 선정은 별도다.

실행 전 JAR를 0700 임시 폴더로 복사하며 size/hash를 확인한다. config와 공개 chain도 snapshot으로 고정하고 원본 위치 기준 상대경로 의미를 유지한다. 비교한 입력과 실행한 입력의 결속을 확인하며 원문 설정·secret 참조 경로를 결과에 넣지 않는다. JVM 옵션·환경변수는 상속하지 않고 stdin을 닫는다. 출력과 시간을 제한하며 원문 stderr나 잘못된 child JSON을 되돌려 쓰지 않는다. timeout은 해당 직접 child를 종료·회수한 뒤 UNKNOWN/exit 6으로 보고한다. 제한 시간은 engine-info와 preflight **각 JVM 호출**에 적용한다. 동일 UID 공격자나 임의 자손 프로세스를 격리하는 sandbox는 아니다.

로컬 metadata 검사는 키 내용을 읽지 않는다. 명시한 엔진 cold 검사는 NIGO가 validator keystore/password와 TLS 자료를 읽어 검증하므로 두 검사를 구별한다. cold 검사는 DB를 열거나 서명·포트 연결·노드 시작을 수행하지 않는다. KEY_MATERIAL=PASS도 실제 peer handshake·인증서 expiry/revocation 인수가 아니다.

## 결과와 남은 인수

engine-info의 올바른 응답은 exit 0이다. 올바른 cold 검사는 NIGO exit 3/INCOMPLETE를 반환하며 BXDL은 **exit 5/INCOMPLETE**로 보존한다. DATABASE_AND_WAL·PORTS_AND_PEERS·NATIVE_RUNTIME의 NOT_CHECKED를 READY로 올리지 않는다. 결합 검사에서도 로컬 product 보고서의 미검사 항목을 엔진 결과로 덮어쓰지 않는다.

2026-09-18에는 NIGO PR #119의 clean 후보를 새로 수신했다. 문서 checkout은 `49d1cefc…`, 공급 JAR source는 `303e163a…`이며 서로 다르다. 공급자는 이 JAR의 QBFT 종료·source 전환·같은 데이터 재시작 시험을 별도 evidence로 제공했다. [이전 dirty 후보](../contracts/nigo/development-2026-09-17/README.md)의 종료/drain·sync 제한과 당시 소비 결과는 이력으로 보존한다. 새 공급자 시험을 BXDL의 JRE/package·launchd·오프라인·4-validator G1-M 인수로 승격하지 않는다.

이번 연결은 설정 자동 렌더·PKI 생성·설치 등록·instance journal·init·launchd를 제공하지 않는다. 정상 cold 결과 뒤에도 초기화·기동·정상 종료·동일 DB 재시작은 별도 구현과 인수가 필요하다. clean 수신으로 NIGO 요구 원장의 OPEN을 변경하거나 정식 release를 승인하지 않는다.
