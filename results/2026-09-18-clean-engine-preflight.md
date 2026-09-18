# Clean 엔진 수신과 제품/native 결합 preflight — 2026-09-18

기반은 BXDL PR #2 `cb97b2713719f1d5948016c27f5f96dfb77b959d`다. 이번 작업 트리의 Rust 코드·테스트를 아래 실행파일과 연결해 검증했다. NIGO의 공유 checkout·요구 원장·사용자 서비스/DB/키는 변경하지 않았다. 이번 변경의 원격 CI·commit·PR·정식 release는 수행하지 않았다.

## 후보 수신

- 읽은 인계 문서: NIGO main `49d1cefc` / #119, `requirements/REQ-0002-clean-candidate-handoff.md`.
- source: `303e163a9b3f293fa39e42d02b8daa1843973c14`, **dirty=false**. 문서 게시 commit과 엔진 source는 다르다.
- 실제 JAR: 159,988,739 bytes / SHA-256 `dd5a366ee990d58ff4fa7da89812f66ec9025443229027bffe1b648cab6d6455`.
- 인계 archive: 153,788,572 bytes / SHA-256 `eef353dd139869489469e9f1eb527719aa8b5b1df147a46d0c4226eb49441707`.
- contract fingerprint: `671dc4052b750b13255804e8aa208f37044b7a0fc2800013ae139884edb3ca13`.
- 실제 archive/JAR pin, inventory 15개, JAR build/console/계약 입력 4개 및 framed aggregate, 공급 source 원문과 저장 engine-info를 교차 확인했다.

[새 snapshot](../contracts/nigo/development-clean-2026-09-18/README.md)은 원문·선별된 공급자 evidence를 보존한다. 기존 dirty 후보 원문·lock·결과는 덮어쓰지 않았다. 최종 확인 시 공유 NIGO checkout에는 별도 GC 작업의 변경이 있었으며, 이번 실행은 계속 보존된 clean 후보 bytes에 고정했다. 이후 작업 트리를 새 후보나 이번 시험 근거로 취급하지 않는다. 수신 owner는 BXDL 제품 구현 작업이며 날짜는 이 문서 날짜다. 실행/종료 계약과 UNKNOWN의 의미를 이번 adapter 계획으로 수용하지만 상세 전체 API 합의·제품 인수 완료나 REQ-0002의 완료 판정으로 확대하지 않는다. NIGO 원장의 소비자 응답을 대신 게시하지 않았다.

## 구현과 회귀

`bxdl preflight --config instance.json --engine-config node.json --jar ... --java ... --lock ... --allow-development`를 추가했다. 전체 engine 옵션이 없으면 기존 metadata-only 명령이다. 일부 옵션만 지정하면 인자 오류다.

제품의 원본 bytes/hash와 native snapshot을 연결하고, 명시 QBFT/VALIDATOR/MTLS, node ID, chain/data/backend, HTTP/P2P endpoint, 여섯 secret 참조가 일치해야 JVM을 실행한다. instanceId는 제품 전용이며 peer/membership/validator ID·pin/sync 정책은 native 입력과 NIGO의 검증을 따른다. 제품/native config 자동 렌더는 하지 않는다.

| 검증 | 결과 |
| --- | --- |
| `make check` | fmt, Clippy `--all-targets -- -D warnings`, 116 tests PASS |
| library | 96 PASS. 새 product binding 테스트 9개 포함 |
| CLI / integration / setup CLI / safety | 8 / 2 / 5 / 5 PASS |
| `make build`, `make build-macos` | host + aarch64-apple-darwin release PASS |
| `make check-linux` | x86_64-unknown-linux-gnu 타입 검사 PASS; Linux 실행 인수 아님 |

회귀에는 서로 다른 상대경로 기준, 명시 필드 전체의 누락·불일치와 JVM 미실행, INSTANT·observer·다른 반환 node/backend 거부, 제품 자료 누락/FAIL, IP·port·hex 대소문자 값 비교, 호출 사이 제품 변경, 부분 CLI 옵션 거부를 포함한다. Mac 충돌 방지용 Unicode/case 비교를 긍정적 동일성 근거로 쓰지 않도록 실제 inode·미존재 suffix를 추가 대조했다.

## 실제 clean JAR cold 검사

최종 CLI에서 9건의 기대 결과를 확인했다. 시험 환경은 macOS 26.6.2 arm64, 로컬 Oracle Java 21.0.7 기반 jlink runtime이다. 해당 Java 실행파일 SHA-256은 `94b66f2cc8edfba9e0e8b25abe93e4c3617d2cc1af7e8eaec0f5b2afe0df5c12`다. 고객용 runtime 선정·재배포 승인·NOTICE/SBOM 완료가 아니다.

- engine-info: exit 0, clean source/console/contract 전체 identity 일치.
- INSTANT h2/rocksdb: 각각 별도 두 cwd와 빈 PATH에서 exit 5 / engine exit 3 / INCOMPLETE.
- 잘못된 JAR pin·이전 후보 lock: `ENGINE_PIN_MISMATCH`.
- dirty identity·새 JAR pin에 이전 전체 identity를 혼합한 lock: `ENGINE_IDENTITY_MISMATCH`.
- 입력·lock·cwd·실행파일/소스 불변, data 미생성 확인.

이 INSTANT 검사를 QBFT key/네트워크 검증으로 사용하지 않았다.

## 설치한 패키지의 실제 QBFT 결합 검사

동일한 최종 CLI·clean JAR·시험 JRE로 새 archive를 조립하고 새 폴더에 설치했다. 설치된 CLI/runtime으로 PATH를 비우고 저장소/설정과 다른 cwd에서 다음을 확인했다.

| 작업 | 결과 |
| --- | --- |
| package build / install / engine inspect | exit 0 |
| 제품/native QBFT 결합 preflight | exit 5 / INCOMPLETE / `configurationBinding=MATCHED` |
| NIGO CONFIGURATION / KEY_MATERIAL | PASS / PASS |
| DATABASE_AND_WAL / PORTS_AND_PEERS / NATIVE_RUNTIME | 모두 NOT_CHECKED 유지 |
| 다른 native HTTP port와 제품 설정 결합 | exit 3 / ENGINE_PRODUCT_MISMATCH |
| engine 옵션 없는 제품 preflight | 기존 exit 5 / ENGINE_PREFLIGHT_NOT_RUN |

Fixture는 새 시험용 NGVK 키 4개의 공개 validator membership, 한 노드의 native 설정과 self-signed TLS key/trust store다. 공급 JAR 내 API와 JDK helper/keytool로 시험 자료만 생성했으며 NIGO 소스를 빌드하지 않았다. peer는 0개, sync/auto-start는 꺼져 있다. 실제 keystore/password/TLS 로딩을 확인하기 위한 **한 validator cold fixture**이고, 4-node 네트워크·거래·handshake·expiry·pin·native/DB 실행·공식 provisioning 기능의 인수가 아니다. 시험 자료는 비공개 작업 폴더에만 보관하며 이 저장소에는 넣지 않는다.

제품/native/chain hash는 실제 입력과 일치했다. 입력 파일 bytes는 불변이며 data 디렉터리가 생성되지 않았다. 사용자 node/data/PKI에 접근하지 않았다. 엔진 inspect/preflight 외 init/run/stop·서비스 명령은 호출하지 않았다.

| 산출물 식별 | 값 |
| --- | --- |
| host 및 설치 CLI SHA-256 | `9443d46a69a5d289a39ad35634efa92c9dc4cc7f9f1d959f9426c3bdd071178e` |
| 시험 BXDL archive SHA-256 | `7f0af8e0788c58c537e1351dea31cd1e8d53a29ebc552f3d926d5382da96ac86` |
| archive 크기 | 180,913,160 bytes |
| 설치 payload | 138 files / 248,203,638 bytes |

시험 archive는 unsigned-development 명시 opt-in으로 사용했으며 배포하지 않았다. 선별된 package 결과는 [기계 판독 요약](./2026-09-18-clean-engine-preflight.json)을 따른다. 기록한 hash는 이 정확한 시험 산출물의 식별이며 후속 재빌드에 재사용하지 않는다.

## 남은 작업과 담당

1. BXDL은 검증한 package/engine lock과 제품/native 자료를 instance 등록·작업 기록으로 묶고 명시 init/resume-init을 연결한다. exit/stdout/같은 attempt terminal report가 일치해야 완료로 기록한다.
2. source-free QBFT 공개 chain/membership, node/peer/pin과 PKI 발급·배치, 거래 및 동일 finalized 높이 비교 fixture는 BXDL 제품 작업이 준비할 후속 범위다. 현재 시험 helper를 고객 생성 도구로 채택하지 않는다. 엔진 변경이 필요하면 별도 NIGO worktree와 공급 계약으로 진행한다.
3. clean 후보의 종료 계약으로 launchd start/status/stop을 구현하고 정확한 process·report·closure fields를 결합해 판정한다. cold timeout의 직접 child 회수 로직을 운영 stop으로 복사하지 않는다.
4. 정식 JRE·OS 지원 범위를 선정하고 exact Mac package의 오프라인·동일 DB/key/WAL 재시작·4-validator G1-M을 검증한다. Linux/systemd·Docker는 별도 후속이다.

NIGO clean 공급과 종료·동기화 보완이 제공되지 않았다는 이전 대기는 해소됐다. 현재 제한은 BXDL의 제품/runtime 통합·운영 fixture·플랫폼 인수이며 개발 후보의 존재를 제품 READY로 표시하지 않는다.
